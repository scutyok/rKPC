//******************************************************************/
//
// Bounding Volume Hierarchy (BVH) for spatial partitioning.
//
// Reorganized for Lithtech-style consistency. 
// Uses a top-down object-median split on the largest axis.
//
//******************************************************************/

use super::Aabb;
use cgmath::Vector3;

//******************************************************************/
//
// Data Structures
//
//******************************************************************/

#[derive(Clone)]
pub struct Bvh {
    pub nodes: Vec<Node>,        // Linearized tree nodes
    pub tri_indices: Vec<usize>, // Global triangle index buffer
}

#[derive(Clone, Debug)]
pub struct Node {
    pub aabb: Aabb,
    pub start: usize,         // Index into tri_indices (leaf only)
    pub count: usize,         // Number of triangles (0 if internal)
    pub left: Option<usize>,  // Left child index
    pub right: Option<usize>, // Right child index
}

//******************************************************************/
//
// Main Implementation
//
//******************************************************************/

impl Bvh {
    /// Builds a new BVH from vertex positions and primitive indices.
    pub fn new(positions: &[Vector3<f32>], indices: &[u32]) -> Self {
        let tri_count = indices.len() / 3;
        let mut tri_indices: Vec<usize> = (0..tri_count).collect();
        let mut nodes: Vec<Node> = Vec::new();
        let mut tri_storage: Vec<usize> = Vec::with_capacity(tri_count);

        if tri_count == 0 {
            return Bvh { nodes, tri_indices: tri_storage };
        }

        Self::build_recursive(
            positions,
            indices,
            &mut tri_indices[..],
            &mut nodes,
            &mut tri_storage,
        );

        Bvh { nodes, tri_indices: tri_storage }
    }

    /// Recursively splits geometry into a tree of AABBs.
    fn build_recursive(
        positions: &[Vector3<f32>],
        indices: &[u32],
        tri_indices: &mut [usize],
        nodes: &mut Vec<Node>,
        tri_storage: &mut Vec<usize>,
    ) -> usize {
        
        // ─── 1. Compute Node AABB ────────────────────────────────────────

        let node_aabb = tri_indices
            .iter()
            .map(|&ti| Self::compute_tri_aabb(positions, indices, ti))
            .fold(None, |acc: Option<Aabb>, aabb| {
                Some(match acc {
                    None => aabb,
                    Some(prev) => Self::merge_aabb(&prev, &aabb),
                })
            })
            .unwrap_or(Aabb { 
                min: Vector3::new(0.0, 0.0, 0.0), 
                max: Vector3::new(0.0, 0.0, 0.0) 
            });

        let node_index = nodes.len();
        nodes.push(Node {
            aabb: node_aabb.clone(),
            start: 0,
            count: 0,
            left: None,
            right: None,
        });

        // ─── 2. Leaf Threshold Check ────────────────────────────────────

        if tri_indices.len() <= 8 {
            let start = tri_storage.len();
            tri_storage.extend_from_slice(tri_indices);
            
            let node = &mut nodes[node_index];
            node.start = start;
            node.count = tri_indices.len();
            return node_index;
        }

        // ─── 3. Split Strategy (Object Median) ──────────────────────────

        let ext = node_aabb.max - node_aabb.min;
        let axis = if ext.x >= ext.y && ext.x >= ext.z { 0 } 
                   else if ext.y >= ext.x && ext.y >= ext.z { 1 } 
                   else { 2 };

        tri_indices.sort_unstable_by(|&a, &b| {
            let ca = Self::compute_centroid(positions, indices, a);
            let cb = Self::compute_centroid(positions, indices, b);
            
            let va = if axis == 0 { ca.x } else if axis == 1 { ca.y } else { ca.z };
            let vb = if axis == 0 { cb.x } else if axis == 1 { cb.y } else { cb.z };
            
            va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
        });

        // ─── 4. Recurse ──────────────────────────────────────────────────

        let mid = tri_indices.len() / 2;
        let (left_slice, right_slice) = tri_indices.split_at_mut(mid);
        
        let left = Self::build_recursive(positions, indices, left_slice, nodes, tri_storage);
        let right = Self::build_recursive(positions, indices, right_slice, nodes, tri_storage);

        let node = &mut nodes[node_index];
        node.left = Some(left);
        node.right = Some(right);
        
        node_index
    }

    /// Traverses the BVH to find triangles intersecting the given AABB.
    pub fn query_aabb(&self, aabb: &Aabb, out: &mut Vec<usize>) {
        if self.nodes.is_empty() { return; }
        
        let mut stack: Vec<usize> = Vec::with_capacity(32);
        stack.push(0);

        while let Some(idx) = stack.pop() {
            let node = &self.nodes[idx];
            
            if !node.aabb.intersects(aabb) {
                continue;
            }

            if node.count > 0 {
                // Leaf: add triangles to output
                for i in 0..node.count {
                    out.push(self.tri_indices[node.start + i]);
                }
            } else {
                // Internal: push children
                if let Some(r) = node.right { stack.push(r); }
                if let Some(l) = node.left { stack.push(l); }
            }
        }
    }

    //**************************************************************
    // Internal Math Helpers
    //**************************************************************

    fn compute_tri_aabb(pos: &[Vector3<f32>], idx: &[u32], t: usize) -> Aabb {
        let a = pos[idx[t * 3] as usize];
        let b = pos[idx[t * 3 + 1] as usize];
        let c = pos[idx[t * 3 + 2] as usize];
        
        Aabb {
            min: Vector3::new(a.x.min(b.x).min(c.x), a.y.min(b.y).min(c.y), a.z.min(b.z).min(c.z)),
            max: Vector3::new(a.x.max(b.x).max(c.x), a.y.max(b.y).max(c.y), a.z.max(b.z).max(c.z)),
        }
    }

    fn merge_aabb(a: &Aabb, b: &Aabb) -> Aabb {
        Aabb {
            min: Vector3::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y), a.min.z.min(b.min.z)),
            max: Vector3::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y), a.max.z.max(b.max.z)),
        }
    }

    fn compute_centroid(pos: &[Vector3<f32>], idx: &[u32], t: usize) -> Vector3<f32> {
        let a = pos[idx[t * 3] as usize];
        let b = pos[idx[t * 3 + 1] as usize];
        let c = pos[idx[t * 3 + 2] as usize];
        (a + b + c) * (1.0 / 3.0)
    }
}