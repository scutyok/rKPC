use cgmath::{Vector3, InnerSpace};

// Reduce iterations to cut CPU cost; still iterative but cheaper.
const MAX_INTERSECT_PUSHBACK_ITERATIONS: usize = 12;
const EXTRA_PENETRATION_ADD: f32 = 0.02;
// Step-up parameters (stairs)
const STEP_HEIGHT: f32 = 0.9; // Match main.rs for reliable stepping and ground detection
const STEP_CLEARANCE: f32 = 0.02;

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
	pub min: Vector3<f32>,
	pub max: Vector3<f32>,
}

impl Aabb {
	pub fn contains(&self, point: Vector3<f32>) -> bool {
		point.x >= self.min.x && point.x <= self.max.x &&
		point.y >= self.min.y && point.y <= self.max.y &&
		point.z >= self.min.z && point.z <= self.max.z
	}

	pub fn intersects(&self, other: &Aabb) -> bool {
		!(self.max.x < other.min.x || self.min.x > other.max.x ||
		  self.max.y < other.min.y || self.min.y > other.max.y ||
		  self.max.z < other.min.z || self.min.z > other.max.z)
	}

	pub fn intersects_line_segment(&self, l0: Vector3<f32>, l1: Vector3<f32>) -> bool {
		let l = l1 - l0;
		let t = (l0 + l1) - (self.max + self.min);
		let e = self.max - self.min;

		if t.x.abs() > l.x.abs() + e.x { return false; }
		if t.y.abs() > l.y.abs() + e.y { return false; }
		if t.z.abs() > l.z.abs() + e.z { return false; }

		if (t.y * l.z - t.z * l.y).abs() > e.y * l.z.abs() + e.z * l.y.abs() { return false; }
		if (t.z * l.x - t.x * l.z).abs() > e.x * l.z.abs() + e.z * l.x.abs() { return false; }
		if (t.x * l.y - t.y * l.x).abs() > e.x * l.y.abs() + e.y * l.x.abs() { return false; }

		true
	}
}

mod bvh;

#[derive(Clone, Copy, Debug)]
pub struct PhysicsSphere {
	pub center: Vector3<f32>,
	pub radius: f32,
}

/// Height provider trait used by the engine to query ground height.
pub trait HeightProvider: Send + Sync {
	fn ground_height(&self, x: f32, y: f32, current_z: Option<f32>) -> f32;
}

/// Simple flat ground provider.
pub struct FlatGround;
impl HeightProvider for FlatGround {
	fn ground_height(&self, _x: f32, _y: f32, _current_z: Option<f32>) -> f32 { 0.0 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerMode { Flying, Walk }

/// Ensure the player stays above ground (simple vertical correction).
pub fn resolve_player_collision(pos: &mut Vector3<f32>, height_provider: &dyn HeightProvider, player_radius: f32, _step_height: f32) {
	let ground_z = height_provider.ground_height(pos.x, pos.y, Some(pos.z));
	let min_z = ground_z + player_radius;
	if pos.z < min_z {
		let diff = min_z - pos.z;
		// Small corrections (thin floors, normal ground): snap instantly.
		// Large corrections (> 0.5): cap to avoid teleporting through ceilings.
		if diff <= 0.5 {
			pos.z = min_z;
		} else {
			pos.z += 0.3;
		}
	}
}

#[derive(Clone)]
pub struct MeshHeightProvider {
	pub positions: Vec<Vector3<f32>>,
	pub indices: Vec<u32>,
	// Precomputed triangle centroids (xy) and XY radius for cheap early-out
	pub tri_centroids: Vec<Vector3<f32>>,
	pub tri_radius_xy: Vec<f32>,
	// Optional BVH for fast triangle queries
	pub bvh: Option<bvh::Bvh>,
}

impl MeshHeightProvider {
	pub fn new(positions: Vec<Vector3<f32>>, indices: Vec<u32>) -> Self {
		// Precompute per-triangle centroid and radius in XY
		let mut tri_centroids = Vec::new();
		let mut tri_radius_xy = Vec::new();
		let tri_count = indices.len() / 3;
		for t in 0..tri_count {
			let i0 = indices[t*3] as usize;
			let i1 = indices[t*3 + 1] as usize;
			let i2 = indices[t*3 + 2] as usize;
			if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
				tri_centroids.push(Vector3::new(0.0,0.0,0.0));
				tri_radius_xy.push(0.0);
				continue;
			}
			let a = positions[i0];
			let b = positions[i1];
			let c = positions[i2];
			let centroid = Vector3::new((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0, (a.z + b.z + c.z) / 3.0);
			let mut radius = 0.0f32;
			for v in [a, b, c].iter() {
				let dx = v.x - centroid.x;
				let dy = v.y - centroid.y;
				radius = radius.max((dx*dx + dy*dy).sqrt());
			}
			tri_centroids.push(centroid);
			tri_radius_xy.push(radius);
		}

		let bvh = if (indices.len() / 3) > 0 {
			Some(bvh::Bvh::new(&positions, &indices))
		} else { None };

		Self { positions, indices, tri_centroids, tri_radius_xy, bvh }
	}

	fn point_in_tri_2d(px: f32, py: f32, a: Vector3<f32>, b: Vector3<f32>, c: Vector3<f32>) -> bool {
		let v0x = c.x - a.x; let v0y = c.y - a.y;
		let v1x = b.x - a.x; let v1y = b.y - a.y;
		let v2x = px - a.x;  let v2y = py - a.y;

		let dot00 = v0x * v0x + v0y * v0y;
		let dot01 = v0x * v1x + v0y * v1y;
		let dot02 = v0x * v2x + v0y * v2y;
		let dot11 = v1x * v1x + v1y * v1y;
		let dot12 = v1x * v2x + v1y * v2y;

		let denom = dot00 * dot11 - dot01 * dot01;
		if denom.abs() < 1e-6 { return false; }
		let inv_denom = 1.0 / denom;
		let u = (dot11 * dot02 - dot01 * dot12) * inv_denom;
		let v = (dot00 * dot12 - dot01 * dot02) * inv_denom;
		(u >= -1e-4) && (v >= -1e-4) && (u + v <= 1.0 + 1e-4)
	}

	fn setup_box(
		&self,
		p0: Vector3<f32>,
		p1: Vector3<f32>,
		dims: Vector3<f32>,
		offset: Vector3<f32>,
		radius: f32,
	) -> Option<(PhysicsSphere, PhysicsSphere, PhysicsSphere, Aabb)> {
		let v = p1 - p0;
		if v.magnitude2() < 0.0001 { return None; }

		let offset_pos0 = p0 + offset;
		let offset_pos1 = p1 + offset;

		let start_sphere = PhysicsSphere { center: offset_pos0, radius };
		let end_sphere = PhysicsSphere { center: offset_pos1, radius };
		let whole_sphere = PhysicsSphere { center: start_sphere.center + v * 0.5, radius: radius + v.magnitude() };

		let mut bmin = offset_pos0;
		let mut bmax = offset_pos0;
		bmin.x = bmin.x.min(offset_pos1.x);
		bmin.y = bmin.y.min(offset_pos1.y);
		bmin.z = bmin.z.min(offset_pos1.z);
		bmax.x = bmax.x.max(offset_pos1.x);
		bmax.y = bmax.y.max(offset_pos1.y);
		bmax.z = bmax.z.max(offset_pos1.z);

		bmin -= dims;
		bmax += dims;

		let boxa = Aabb { min: bmin, max: bmax };

		Some((start_sphere, end_sphere, whole_sphere, boxa))
	}

	/// More complete iterative wall-collision resolver based on the original engine.
	pub fn resolve_player_movement(&self, prev_pos: Vector3<f32>, pos: &mut Vector3<f32>, radius: f32) {
		// Make the player's collision box slightly smaller and less tall
		// (narrower horizontally and reduced vertical extent).
		let dims = Vector3::new(radius * 0.8, radius * 0.8, radius * 0.5);
		let offset = Vector3::new(0.0, 0.0, 0.0);

		let setup = self.setup_box(prev_pos, *pos, dims, offset, radius);
		if setup.is_none() { return; }
		let (_start_sphere, _end_sphere, _whole_sphere, boxa) = setup.unwrap();

		let idx = &self.indices;
		let posv = &self.positions;

		// gather candidate triangles via BVH if available
		let mut candidates: Vec<usize> = Vec::new();
		if let Some(ref b) = self.bvh {
			b.query_aabb(&boxa, &mut candidates);
		} else {
			candidates = (0..(idx.len() / 3)).collect();
		}

		for _iter in 0..MAX_INTERSECT_PUSHBACK_ITERATIONS {
			let mut moved = false;

			for &t in candidates.iter() {
				let i0 = idx[t*3] as usize;
				let i1 = idx[t*3 + 1] as usize;
				let i2 = idx[t*3 + 2] as usize;
				if i0 >= posv.len() || i1 >= posv.len() || i2 >= posv.len() { continue; }
				let a = posv[i0];
				let b = posv[i1];
				let c = posv[i2];

				// quick AABB check
				let tri_min = Vector3::new(a.x.min(b.x).min(c.x), a.y.min(b.y).min(c.y), a.z.min(b.z).min(c.z));
				let tri_max = Vector3::new(a.x.max(b.x).max(c.x), a.y.max(b.y).max(c.y), a.z.max(b.z).max(c.z));
				let tri_box = Aabb { min: tri_min - Vector3::new(radius, radius, radius), max: tri_max + Vector3::new(radius, radius, radius) };
				if !tri_box.intersects(&boxa) { continue; }

				// Cheap 2D centroid radius test to avoid expensive closest-point if far away
				let centroid = self.tri_centroids[t];
				let tri_radius = self.tri_radius_xy[t];
				let dx_c = pos.x - centroid.x;
				let dy_c = pos.y - centroid.y;
				let dist2 = dx_c*dx_c + dy_c*dy_c;
				let early_radius = (radius + tri_radius + 0.5) * (radius + tri_radius + 0.5);
				if dist2 > early_radius { continue; }

				let edge1 = b - a;
				let edge2 = c - a;
				let n = edge1.cross(edge2);
				let n_mag = n.magnitude();
				if n_mag < 1e-6 { continue; } // degenerate triangle
				let normal = n / n_mag;

				// Floor/ceiling triangles (normal mostly vertical) are handled
				// by ground_height; skip horizontal pushback for them.
				if normal.z.abs() > 0.7 { continue; }

				// Wall-like triangle — check vertical overlap with player.
				// Player eye/camera is at pos.z; feet are ~0.5 below.
				const EYE_HEIGHT: f32 = 0.5;
				let foot_z = pos.z - EYE_HEIGHT;
				let tri_min_z = a.z.min(b.z).min(c.z);
				let tri_max_z = a.z.max(b.z).max(c.z);
				// Skip walls entirely above the player's head or below their feet
				if foot_z > tri_max_z + 0.15 || pos.z + 0.1 < tri_min_z { continue; }

				let closest = closest_point_on_triangle(*pos, a, b, c);
				let dx = pos.x - closest.x;
				let dy = pos.y - closest.y;
				let horiz_dist = (dx*dx + dy*dy).sqrt();

				// Only process if within collision radius
				if horiz_dist >= radius { continue; }

				// --- Step-up check ---
				// If the wall top is within STEP_HEIGHT of player feet,
				// step up over it instead of pushing back.
				let step_delta = tri_max_z - foot_z;
				if step_delta > 0.01 && step_delta <= STEP_HEIGHT {
					let new_foot_z = tri_max_z + STEP_CLEARANCE;
					let stepped_z = new_foot_z + EYE_HEIGHT;

					// Quick headroom check for taller steps
					let mut head_clear = true;
					if step_delta > 0.35 {
						let head_aabb = Aabb {
							min: Vector3::new(pos.x - radius * 0.6, pos.y - radius * 0.6, stepped_z),
							max: Vector3::new(pos.x + radius * 0.6, pos.y + radius * 0.6, stepped_z + 0.8),
						};
						let mut head_overlaps: Vec<usize> = Vec::new();
						if let Some(ref bv) = self.bvh {
							bv.query_aabb(&head_aabb, &mut head_overlaps);
						} else {
							head_overlaps = (0..(idx.len() / 3)).collect();
						}
						for &ot in head_overlaps.iter() {
							if ot == t { continue; }
							let oi0 = idx[ot*3] as usize;
							let oi1 = idx[ot*3 + 1] as usize;
							let oi2 = idx[ot*3 + 2] as usize;
							if oi0 >= posv.len() || oi1 >= posv.len() || oi2 >= posv.len() { continue; }
							let oa = posv[oi0]; let ob = posv[oi1]; let oc = posv[oi2];
							let oe1 = ob - oa; let oe2 = oc - oa;
							let on = oe1.cross(oe2);
							let omag = on.magnitude();
							if omag < 1e-6 { continue; }
							let onorm = on / omag;
							// Only ceiling-like triangles block headroom
							if onorm.z > -0.5 { continue; }
							let omin_z = oa.z.min(ob.z).min(oc.z);
							if omin_z > stepped_z - 0.05 && omin_z < stepped_z + 0.8 {
								head_clear = false;
								break;
							}
						}
					}

					if head_clear {
						pos.z = stepped_z;
						continue; // skip pushback, allow horizontal movement
					}
				}

				// --- Horizontal pushback ---
				if horiz_dist > 1e-6 {
					let push = (radius - horiz_dist + EXTRA_PENETRATION_ADD).max(0.0);
					pos.x += dx / horiz_dist * push;
					pos.y += dy / horiz_dist * push;
					moved = true;
				} else {
					// Exactly overlapping — push along movement direction
					let mvx = pos.x - prev_pos.x;
					let mvy = pos.y - prev_pos.y;
					let mv_len = (mvx*mvx + mvy*mvy).sqrt();
					if mv_len > 1e-6 {
						pos.x += mvx / mv_len * (radius * 0.5);
						pos.y += mvy / mv_len * (radius * 0.5);
						moved = true;
					}
				}
			}

			if !moved { break; }
		}
	}
}

fn closest_point_on_triangle(p: Vector3<f32>, a: Vector3<f32>, b: Vector3<f32>, c: Vector3<f32>) -> Vector3<f32> {
	let ab = b - a;
	let ac = c - a;
	let ap = p - a;

	let d1 = ab.dot(ap);
	let d2 = ac.dot(ap);
	if d1 <= 0.0 && d2 <= 0.0 { return a; }

	let bp = p - b;
	let d3 = ab.dot(bp);
	let d4 = ac.dot(bp);
	if d3 >= 0.0 && d4 <= d3 { return b; }

	let vc = d1 * d4 - d3 * d2;
	if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
		let v = d1 / (d1 - d3);
		return a + ab * v;
	}

	let cp = p - c;
	let d5 = ab.dot(cp);
	let d6 = ac.dot(cp);
	if d6 >= 0.0 && d5 <= d6 { return c; }

	let vb = d5 * d2 - d1 * d6;
	if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
		let w = d2 / (d2 - d6);
		return a + ac * w;
	}

	let va = d3 * d6 - d5 * d4;
	if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
		let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
		return b + (c - b) * w;
	}

	let denom = 1.0 / (va + vb + vc);
	let v = vb * denom;
	let w = vc * denom;
	a + ab * v + ac * w
}

impl HeightProvider for MeshHeightProvider {
	fn ground_height(&self, x: f32, y: f32, current_z: Option<f32>) -> f32 {
		let mut best: Option<f32> = None;
		let idx = &self.indices;
		let pos = &self.positions;

		// query a small XY box around point to limit triangles
		let query_box = Aabb {
			min: Vector3::new(x - 1.0, y - 1.0, -1000.0),
			max: Vector3::new(x + 1.0, y + 1.0, 1000.0),
		};

		let mut candidates: Vec<usize> = Vec::new();
		if let Some(ref b) = self.bvh {
			b.query_aabb(&query_box, &mut candidates);
		} else {
			candidates = (0..(idx.len() / 3)).collect();
		}

		for &t in candidates.iter() {
			let i0 = idx[t*3] as usize;
			let i1 = idx[t*3 + 1] as usize;
			let i2 = idx[t*3 + 2] as usize;
			if i0 >= pos.len() || i1 >= pos.len() || i2 >= pos.len() { continue; }
			let v0 = pos[i0];
			let v1 = pos[i1];
			let v2 = pos[i2];

			if MeshHeightProvider::point_in_tri_2d(x, y, v0, v1, v2) {
				let edge1 = v1 - v0;
				let edge2 = v2 - v0;
				let n = edge1.cross(edge2);
				// Accept both upward and downward facing normals (thin floors
				// may only have one winding). Skip near-vertical triangles.
				if n.z.abs() < 1e-6 { continue; }
				let z = v0.z - (n.x * (x - v0.x) + n.y * (y - v0.y)) / n.z;
				if let Some(curz) = current_z {
					// Skip floors more than 0.6 above player (don't detect upper stories)
					if z > curz + 0.6 { continue; }
				}
				// Pick the highest floor that is at or below the player
				// (i.e. the floor we're actually standing on, not one far below)
				best = Some(match best {
					None => z,
					Some(prev_best) => {
						if let Some(curz) = current_z {
							// Both below player → pick higher one (closer to feet)
							// One above, one below → pick the one closer to player Z
							let da = (curz - z).abs();
							let db = (curz - prev_best).abs();
							if da < db { z } else { prev_best }
						} else {
							prev_best.max(z)
						}
					}
				});
			}
		}
		best.unwrap_or(0.0)
	}
}


