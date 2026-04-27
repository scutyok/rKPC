//******************************************************************/
// Occlusion culling module for the KissEngine renderer.
//
// Provides frustum culling and basic occlusion culling for draw groups.
// Each `DrawGroup` gets a precomputed AABB. Before issuing draw calls,
// the culling system tests each group's AABB against the camera frustum
// to skip geometry that is entirely off-screen.
//
//******************************************************************/

use cgmath::{Matrix4, Vector3, Vector4};
use rayon::prelude::*;

//******************************************************************/
//
// Axis-aligned bounding box for a draw group.
//
//******************************************************************/

#[derive(Clone, Copy, Debug, Default)]
pub struct GroupAabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl GroupAabb {
    /// Build an AABB from a slice of vertex positions (each vertex is [x, y, z]).
    pub fn from_positions(positions: &[[f32; 3]]) -> Self {
        if positions.is_empty() {
            return Self::default();
        }
        let mut min = positions[0];
        let mut max = positions[0];
        for p in positions.iter().skip(1) {
            min[0] = min[0].min(p[0]);
            min[1] = min[1].min(p[1]);
            min[2] = min[2].min(p[2]);
            max[0] = max[0].max(p[0]);
            max[1] = max[1].max(p[1]);
            max[2] = max[2].max(p[2]);
        }
        Self { min, max }
    }

    // All 8 corner points of the AABB.
    pub fn corners(&self) -> [Vector3<f32>; 8] {
        let [x0, y0, z0] = self.min;
        let [x1, y1, z1] = self.max;
        [
            Vector3::new(x0, y0, z0),
            Vector3::new(x1, y0, z0),
            Vector3::new(x0, y1, z0),
            Vector3::new(x1, y1, z0),
            Vector3::new(x0, y0, z1),
            Vector3::new(x1, y0, z1),
            Vector3::new(x0, y1, z1),
            Vector3::new(x1, y1, z1),
        ]
    }

    // Center point of the AABB.
    pub fn center(&self) -> Vector3<f32> {
        Vector3::new(
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        )
    }

    // Half-extents of the AABB.
    pub fn half_extents(&self) -> Vector3<f32> {
        Vector3::new(
            (self.max[0] - self.min[0]) * 0.5,
            (self.max[1] - self.min[1]) * 0.5,
            (self.max[2] - self.min[2]) * 0.5,
        )
    }
}

//******************************************************************/
//
// A plane in the form `ax + by + cz + d = 0`, with (a,b,c) pointing inward.
//
//******************************************************************/

#[derive(Clone, Copy, Debug, Default)]
pub struct Plane {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
}

impl Plane {
    // Signed distance from a point to this plane (positive = inside frustum).
    pub fn distance(&self, p: Vector3<f32>) -> f32 {
        self.a * p.x + self.b * p.y + self.c * p.z + self.d
    }

    // Normalize the plane so that (a,b,c) is unit length.
    pub fn normalize(&mut self) {
        let len = (self.a * self.a + self.b * self.b + self.c * self.c).sqrt();
        if len > 1e-8 {
            let inv = 1.0 / len;
            self.a *= inv;
            self.b *= inv;
            self.c *= inv;
            self.d *= inv;
        }
    }
}


//******************************************************************/
//
// The 6 planes of a view frustum: left, right, bottom, top, near, far.
//
//******************************************************************/

#[derive(Clone, Debug, Default)]
pub struct Frustum {
    pub planes: [Plane; 6],
}

impl Frustum {
    // Extract frustum planes from a combined view-projection matrix.
    // The matrix should be `proj * view` (column-major, as cgmath uses).
    pub fn from_view_proj(vp: &Matrix4<f32>) -> Self {
        // Gribb-Hartmann method: extract planes from rows of the VP matrix.
        // For column-major (cgmath), row i of M is: m[0][i], m[1][i], m[2][i], m[3][i]
        let row = |i: usize| -> Vector4<f32> {
            Vector4::new(vp[0][i], vp[1][i], vp[2][i], vp[3][i])
        };

        let r0 = row(0);
        let r1 = row(1);
        let r2 = row(2);
        let r3 = row(3);

        let mut planes = [Plane::default(); 6];

        // Left:   r3 + r0
        let p = r3 + r0;
        planes[0] = Plane { a: p.x, b: p.y, c: p.z, d: p.w };

        // Right:  r3 - r0
        let p = r3 - r0;
        planes[1] = Plane { a: p.x, b: p.y, c: p.z, d: p.w };

        // Bottom: r3 + r1
        let p = r3 + r1;
        planes[2] = Plane { a: p.x, b: p.y, c: p.z, d: p.w };

        // Top:    r3 - r1
        let p = r3 - r1;
        planes[3] = Plane { a: p.x, b: p.y, c: p.z, d: p.w };

        // Near:   r3 + r2
        let p = r3 + r2;
        planes[4] = Plane { a: p.x, b: p.y, c: p.z, d: p.w };

        // Far:    r3 - r2
        let p = r3 - r2;
        planes[5] = Plane { a: p.x, b: p.y, c: p.z, d: p.w };

        // Normalize all planes
        for plane in &mut planes {
            plane.normalize();
        }

        Frustum { planes }
    }

    // Test whether an AABB is completely outside the frustum.
    // Returns `true` if the box is fully outside (should be culled).
    // Returns `false` if the box is inside or intersecting (should be drawn).
    pub fn is_aabb_outside(&self, aabb: &GroupAabb) -> bool {
        let center = aabb.center();
        let half = aabb.half_extents();

        for plane in &self.planes {
            // Compute the "effective radius" of the AABB projected onto the plane normal
            let r = half.x * plane.a.abs()
                   + half.y * plane.b.abs()
                   + half.z * plane.c.abs();
            let dist = plane.distance(center);
            // If the AABB is entirely on the negative side of any plane, it's outside
            if dist < -r {
                return true;
            }
        }
        false
    }
}

//******************************************************************/
//
// Occlusion culling state — holds per-group AABBs and provides culling queries.
//
//******************************************************************/

#[derive(Clone, Debug, Default)]
pub struct OcclusionCuller {
    /// One AABB per draw group (indexed same as `draw_groups`).
    pub group_aabbs: Vec<GroupAabb>,
    /// Reusable visibility buffer — `true` = visible, `false` = culled.
    pub visibility: Vec<bool>,
    /// Stats from last cull pass.
    pub last_total: usize,
    pub last_visible: usize,
}

impl OcclusionCuller {
    pub fn new() -> Self {
        Self::default()
    }

    // Clear and rebuild AABBs from the current draw groups.
    // Call this after loading a new world/level.
    //
    // `vertices` — the full vertex position list (just the xyz positions).
    // `indices`  — the full index buffer.
    // `groups`   — list of (first_index, index_count) per draw group.
    pub fn build_from_groups(
        &mut self,
        vertex_positions: &[[f32; 3]],
        indices: &[u32],
        groups: &[(u32, u32)], // (first_index, index_count)
    ) {
        self.group_aabbs = groups
            .par_iter()
            .map(|&(first_index, index_count)| {
                let start = first_index as usize;
                let end = (first_index + index_count) as usize;

                let mut min = [f32::MAX; 3];
                let mut max = [f32::MIN; 3];
                let mut any = false;
                for i in start..end.min(indices.len()) {
                    let vi = indices[i] as usize;
                    if vi < vertex_positions.len() {
                        let p = vertex_positions[vi];
                        if !any {
                            min = p;
                            max = p;
                            any = true;
                        } else {
                            min[0] = min[0].min(p[0]);
                            min[1] = min[1].min(p[1]);
                            min[2] = min[2].min(p[2]);
                            max[0] = max[0].max(p[0]);
                            max[1] = max[1].max(p[1]);
                            max[2] = max[2].max(p[2]);
                        }
                    }
                }
                GroupAabb { min, max }
            })
            .collect();

        self.visibility.resize(self.group_aabbs.len(), true);
    }

    // Run frustum culling against all groups.
    // After calling this, read `self.visibility[group_index]` to decide
    // whether to issue the draw call.
    pub fn cull(&mut self, frustum: &Frustum) {
        self.last_total = self.group_aabbs.len();

        self.visibility = self.group_aabbs
            .par_iter()
            .map(|aabb| !frustum.is_aabb_outside(aabb))
            .collect();

        self.last_visible = self.visibility.iter().filter(|&&v| v).count();
    }

    // Check if a specific draw group should be drawn.
    pub fn is_visible(&self, group_index: usize) -> bool {
        self.visibility.get(group_index).copied().unwrap_or(true)
    }

    // Fraction of groups that were visible in the last cull pass.
    pub fn visible_fraction(&self) -> f32 {
        if self.last_total == 0 {
            1.0
        } else {
            self.last_visible as f32 / self.last_total as f32
        }
    }
}
