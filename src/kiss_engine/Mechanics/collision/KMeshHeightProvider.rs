pub use crate::util::math::Vector3;
pub use crate::util::geometry::*;
pub use crate::util::graph::{Bvh};

pub use crate::collision::clip_poly_to_z_range;
pub use crate::collision::MovingCylinder;
pub use crate::collision::HeightProvider;

//******************************************************************/
//
// MeshHeightProvider
//
//******************************************************************/

const MAX_PHYSICS_ITERATIONS: usize = 40;

// Player cylinder dimensions (Z-up: height along Z, radius in XY)
const PLAYER_HALF_HEIGHT: f32 = 0.35;
const STAIR_ALLOW_UP_LIMIT: f32 = 0.7071; // cos(45°) — steepest walkable surface

#[derive(Clone)]

pub struct MeshHeightProvider {
	pub positions:      Vec<Vector3>,
	pub indices:        Vec<u32>,
	pub tri_centroids:  Vec<Vector3>,
	pub tri_radius_xy:  Vec<f32>,
	pub tri_radius_3d:  Vec<f32>,
	pub bvh:            Option<Bvh>,
}

impl MeshHeightProvider {
	pub fn new(positions: Vec<Vector3>, indices: Vec<u32>) -> Self {
		use rayon::prelude::*;

		let tri_count = indices.len() / 3;

		let tri_data: Vec<(Vector3, f32, f32)> = (0..tri_count)
			.into_par_iter()
			.map(|t| {
				let i0 = indices[t*3]     as usize;
				let i1 = indices[t*3 + 1] as usize;
				let i2 = indices[t*3 + 2] as usize;
				if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
					return (Vector3::new(0.0, 0.0, 0.0), 0.0, 0.0);
				}
				let a = positions[i0];
				let b = positions[i1];
				let c = positions[i2];
				let centroid = (a + b + c) * (1.0 / 3.0);

				let mut rxy = 0.0f32;
				let mut r3d = 0.0f32;
				for v in [a, b, c].iter() {
					let dx = v.x - centroid.x;
					let dy = v.y - centroid.y;
					let dz = v.z - centroid.z;
					rxy = rxy.max((dx * dx + dy * dy).sqrt());
					r3d = r3d.max((dx * dx + dy * dy + dz * dz).sqrt());
				}
				(centroid, rxy, r3d)
			})
			.collect();

		let mut tri_centroids = Vec::with_capacity(tri_count);
		let mut tri_radius_xy = Vec::with_capacity(tri_count);
		let mut tri_radius_3d = Vec::with_capacity(tri_count);
		for (c, rxy, r3d) in tri_data {
			tri_centroids.push(c);
			tri_radius_xy.push(rxy);
			tri_radius_3d.push(r3d);
		}

		let bvh = if tri_count > 0 {
			Some(Bvh::new(&positions, &indices))
		} else { None };

		Self { positions, indices, tri_centroids, tri_radius_xy, tri_radius_3d, bvh }
	}

	fn point_in_tri_2d(px: f32, py: f32, a: Vector3, b: Vector3, c: Vector3) -> bool {
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

	/// Setup swept AABB — adapted from SetupAxisAlignedBox() in collision.cpp.
	fn setup_axis_aligned_box(
		p0: Vector3, p1: Vector3,
		dims: Vector3, offset: Vector3,
	) -> (PhysicsSphere, AABB) {
		let vs = p0 + offset;
		let ve = p1 + offset;

		let b = AABB {
			min: Vector3::new(ve.x - dims.x, ve.y - dims.y, ve.z - dims.z),
			max: Vector3::new(ve.x + dims.x, ve.y + dims.y, ve.z + dims.z),
		};

		let wmin = Vector3::new(vs.x.min(ve.x) - dims.x, vs.y.min(ve.y) - dims.y, vs.z.min(ve.z) - dims.z);
		let wmax = Vector3::new(vs.x.max(ve.x) + dims.x, vs.y.max(ve.y) + dims.y, vs.z.max(ve.z) + dims.z);

		let ws = PhysicsSphere {
			center: (wmin + wmax) * 0.5,
			radius: (wmax - (wmin + wmax) * 0.5).magnitude() + 1.0,
		};

		(ws, b)
	}

	// Cylinder–triangle intersection test for stair stepping.
	// Adapted from StairStep_CylinderPolyIntersect() in collision.cpp.
	// Z-up: base is the bottom of the cylinder, height is full height.
	fn stair_step_cylinder_tri_intersect(
		base: Vector3, cyl_height: f32, cyl_radius: f32,
		verts: &[Vector3; 3], normal: &Vector3,
		tri_center: Vector3, tri_radius_3d: f32,
		result_max_z: &mut f32,
	) -> bool {
		let adj_r = if cyl_radius > 0.1 { cyl_radius - 0.1 } else { cyl_radius };
		let half_h = cyl_height * 0.5;
		let sphere_r = (half_h * half_h + adj_r * adj_r).sqrt();
		let cyl_center = Vector3::new(base.x, base.y, base.z + half_h);
		let cd = cyl_center - tri_center;
		let total_r = sphere_r + tri_radius_3d;
		if cd.dot(&cd) > total_r * total_r { return false; }

		if normal.z.abs() < 0.001 { return false; }
		let plane_dist = normal.dot(&verts[0]);
		let z_intercept = (plane_dist - normal.x * base.x - normal.y * base.y) / normal.z;
		let z_proj = adj_r * ((1.0 / (normal.z * normal.z) - 1.0).max(0.0)).sqrt();
		let plane_max_z = z_intercept + z_proj;

		if plane_max_z < base.z { return false; }

		let plane_min_z = z_intercept - z_proj;
		let min_z = base.z.max(plane_min_z - 0.1);
		let max_z = (base.z + cyl_height).min(plane_max_z + 0.1);

		let mut clipped = Vec::new();
		if !clip_poly_to_z_range(verts, &mut clipped, min_z, max_z) { return false; }

		let r_sqr = adj_r * adj_r;
		let cyl_seg = Segment {
			origin: base,
			direction: Vector3::new(0.0, 0.0, cyl_height),
		};

		let mut edge_intersect = false;
		let mut max_edge_z = f32::MIN;

		// High point on plane
		let pc = Vector3::new(normal.x, normal.y, 0.0);
		let pc_mag = pc.magnitude();
		let high_pt = if pc_mag > 0.001 {
			let pcn = pc * (1.0 / pc_mag);
			Vector3::new(base.x - pcn.x * adj_r, base.y - pcn.y * adj_r, plane_max_z)
		} else {
			Vector3::new(base.x, base.y, plane_max_z)
		};
		let mut hp_inside = true;

		let nv = clipped.len();
		for i in 0..nv {
			let prev = if i == 0 { nv - 1 } else { i - 1 };
			let eseg = Segment {
				origin: clipped[prev],
				direction: clipped[i] - clipped[prev],
			};

			let mut _p0 = 0.0;
			let mut p1_val = 0.0;
			let ds = dist_sqr_seg_seg(&cyl_seg, &eseg, &mut _p0, &mut p1_val);
			if ds < r_sqr {
				edge_intersect = true;
				let elen = eseg.direction.magnitude();
				if elen > 1e-6 {
					let adj = (r_sqr - ds).sqrt() / elen;
					let adj = if eseg.direction.z < 0.0 { -adj } else { adj };
					let mzp = (p1_val + adj).clamp(0.0, 1.0);
					let ez = eseg.origin.z + eseg.direction.z * mzp;
					if ez > max_edge_z { max_edge_z = ez; }
				}
			}

			if hp_inside {
				let w = (clipped[prev].x - high_pt.x) * eseg.direction.y
					  - (clipped[prev].y - high_pt.y) * eseg.direction.x;
				hp_inside = w >= -0.001;
			}
		}

		if !edge_intersect && !hp_inside { return false; }
		*result_max_z = if hp_inside { plane_max_z } else { max_edge_z };
		true
	}

	// Single-segment stair step — adapted from StairStep_Segment() in collision.cpp.
	fn stair_step_segment(
		&self, offset: Vector3,
		p0: &mut Vector3, p1: &mut Vector3,
		dims: Vector3,
	) -> bool {
		let (whole_sphere, boxa) = Self::setup_axis_aligned_box(*p0, *p1, dims, offset);

		// Cylinder parameters from the box
		let half_box_w = (boxa.max.x - boxa.min.x) * 0.5;
		let half_box_d = (boxa.max.y - boxa.min.y) * 0.5;
		let cyl_base   = Vector3::new(boxa.min.x + half_box_w, boxa.min.y + half_box_d, boxa.min.z);
		let mut cyl_height = boxa.max.z - boxa.min.z;
		let cyl_radius = half_box_w.min(half_box_d);

		let mut _down_remaining = if p0.z > p1.z { p0.z - p1.z } else { 0.0 };
		let mut hit_non_step = false;

		// The sphere/box are mutable for iterative adjustments
		let mut ws = whole_sphere;
		let mut bx = boxa;
		let mut cb = cyl_base;
		let mut cur_dims = dims;
		let mut cur_offset = offset;

		// Query BVH once for the initial box
		let mut candidates = Vec::new();
		if let Some(ref bvh) = self.bvh {
			bvh.query_aabb(&bx, &mut candidates);
		} else {
			candidates = (0..(self.indices.len() / 3)).collect();
		}

		for &t in candidates.iter() {
			let i0 = self.indices[t*3]     as usize;
			let i1 = self.indices[t*3 + 1] as usize;
			let i2 = self.indices[t*3 + 2] as usize;
			if i0 >= self.positions.len() || i1 >= self.positions.len() || i2 >= self.positions.len() { continue; }
			let va = self.positions[i0];
			let vb = self.positions[i1];
			let vc = self.positions[i2];

			let edge1 = vb - va;
			let edge2 = vc - va;
			let n = edge1.cross(&edge2);
			let nmag = n.magnitude();
			if nmag < 1e-6 { continue; }
			let normal = n / nmag;

			// Handle both winding orders by flipping so normal points up.
			// Lithtech meshes have arbitrary winding.
			let normal = if normal.z < 0.0 { -normal } else { normal };
			if normal.z <= 0.001 { continue; }

			// Quick sphere cull
			let d = normal.dot(&ws.center) - normal.dot(&va);
			if d.abs() > ws.radius { continue; }

			let tri = [va, vb, vc];
			let tri_center = self.tri_centroids[t];
			let tri_r3d = self.tri_radius_3d[t];

			let mut max_z = 0.0f32;
			if !Self::stair_step_cylinder_tri_intersect(cb, cyl_height, cyl_radius, &tri, &normal, tri_center, tri_r3d, &mut max_z) {
				continue;
			}

			let max_push = max_z - bx.min.z;

			// Guard: only step onto surfaces near foot level (below player center).
			// Ceilings intersecting from above have max_z above the player center.
			let is_floor_level = max_z <= p1.z + 0.05;
			if max_push > 0.0 && is_floor_level && normal.z > STAIR_ALLOW_UP_LIMIT {
				let push = max_push + 0.01;
				let to_add = Vector3::new(0.0, 0.0, push);

				*p1 = *p1 + to_add;
				p0.z = p1.z;

				// Shrink the box (matching C++)
				cur_offset.z -= push * 0.5;
				cur_dims.z -= push * 0.5;
				if cur_dims.z < 0.01 { cur_dims.z = 0.01; }

				let (new_ws, new_bx) = Self::setup_axis_aligned_box(*p0, *p1, cur_dims, cur_offset);
				ws = new_ws;
				bx = new_bx;

				cb.z = bx.min.z;
				cyl_height = bx.max.z - bx.min.z;

				_down_remaining -= push;
			} else if max_push > 0.0 {
				hit_non_step = true;
			}
		}

		hit_non_step
	}

	/// Stair stepping wrapper with anti-tunneling subdivision.
	/// Adapted from StairStep() in collision.cpp.
	fn stair_step(
		&self, offset: Vector3,
		p0: &mut Vector3, p1: &mut Vector3,
		dims: Vector3,
	) -> bool {
		let movement = *p1 - *p0;
		let n_steps = ((movement.z.abs()) / (dims.z * 2.0 - 0.01).max(0.01)).floor() as u32 + 1;
		let step_vec = movement * (1.0 / n_steps as f32);

		let mut cur_p0 = *p0;
		let mut stepped = false;
		let mut hit_non_step = false;

		for _ in 0..n_steps {
			let cur_p1 = cur_p0 + step_vec;
			let mut test_p0 = cur_p0;
			let mut test_p1 = cur_p1;

			hit_non_step |= self.stair_step_segment(offset, &mut test_p0, &mut test_p1, dims);

			if test_p0 != cur_p0 || test_p1 != cur_p1 {
				cur_p0 = test_p1;
				stepped = true;
			} else {
				cur_p0 = cur_p1;
			}
		}

		if stepped { *p1 = cur_p0; }
		hit_non_step
	}

	// Cylinder–mesh collision, adapted from CollideCylinderWithTree() in collision.cpp.
	// Uses BVH instead of BSP; otherwise follows the same segmented traversal.
	fn collide_cylinder_with_mesh(
		&self, p0: Vector3, p1: &mut Vector3,
		offset: Vector3, dims: Vector3,
	) -> (u32, Vector3) {
		let cyl_radius = dims.x.min(dims.y);
		let cyl_height = dims.z;

		if cyl_radius < 0.01 || cyl_height < 0.01 {
			return (0, Vector3::new(0.0, 0.0, 0.0));
		}

		let full_start = p0 + offset;
		let full_end   = *p1 + offset;
		let direction  = full_end - full_start;
		let mut velocity_left = direction.magnitude();
		if velocity_left < 0.001 {
			return (0, Vector3::new(0.0, 0.0, 0.0));
		}
		let dir_norm = direction * (1.0 / velocity_left);

		// Decide segmentation (matching C++)
		let horiz_vel = Vector3::new(direction.x, direction.y, 0.0).magnitude();
		let vert_vel  = direction.z.abs();
		let (vel_step, vel_seg);
		if vert_vel > cyl_height * 1.8 || horiz_vel > cyl_radius * 0.9 {
			let r_ratio = horiz_vel / cyl_radius;
			let h_ratio = vert_vel / (cyl_height * 2.0);
			let (sr, sh);
			if r_ratio > h_ratio {
				sr = cyl_radius;
				sh = (sr / horiz_vel.max(1e-6)) * vert_vel;
			} else {
				sh = cyl_height * 2.0;
				sr = (sh / vert_vel.max(1e-6)) * horiz_vel;
			}
			let fs = (sr * sr + sh * sh).sqrt();
			vel_step = fs * 0.75;
			vel_seg  = fs * 0.9;
		} else {
			vel_step = velocity_left;
			vel_seg  = velocity_left;
		}

		if vel_step <= 0.001 { return (0, Vector3::new(0.0, 0.0, 0.0)); }

		let mut num_hits: u32 = 0;
		let old_hit_count: u32 = 0;
		let mut retry_hit_count: u32 = 0;
		let mut retry_collision: i32 = 3;
		let mut collide_normal = Vector3::new(0.0, 0.0, 0.0);

		let mut cylinder = MovingCylinder::new(full_start, full_end, cyl_radius, cyl_height);
		cylinder.real_end = full_end;

		let mut current_start = full_start;

		while velocity_left > 0.0 {
			if old_hit_count == num_hits {
				let seg_end = if velocity_left < vel_step {
					current_start + dir_norm * velocity_left
				} else {
					current_start + dir_norm * vel_seg
				};
				cylinder.start = current_start;
				cylinder.end = seg_end;
				cylinder.recalc();
			}

			// Query BVH for candidate triangles using movement sphere
			let move_aabb = AABB {
				min: Vector3::new(
					cylinder.move_mid.x - cylinder.move_sphere,
					cylinder.move_mid.y - cylinder.move_sphere,
					cylinder.move_bottom,
				),
				max: Vector3::new(
					cylinder.move_mid.x + cylinder.move_sphere,
					cylinder.move_mid.y + cylinder.move_sphere,
					cylinder.move_top,
				),
			};
			let mut candidates = Vec::new();
			if let Some(ref bvh) = self.bvh {
				bvh.query_aabb(&move_aabb, &mut candidates);
			} else {
				candidates = (0..(self.indices.len() / 3)).collect();
			}

			// Test each candidate triangle (matching BSP per-node traversal)
			for &t in candidates.iter() {
				let i0 = self.indices[t*3]     as usize;
				let i1 = self.indices[t*3 + 1] as usize;
				let i2 = self.indices[t*3 + 2] as usize;
				if i0 >= self.positions.len() || i1 >= self.positions.len() || i2 >= self.positions.len() { continue; }
				let va = self.positions[i0];
				let vb = self.positions[i1];
				let vc = self.positions[i2];

				let edge1 = vb - va;
				let edge2 = vc - va;
				let n = edge1.cross(&edge2);
				let nmag = n.magnitude();
				if nmag < 1e-6 { continue; }
				let normal = n / nmag;
				let plane_dist = normal.dot(&va);

				// Make triangles double-sided: flip normal to face toward
				// the cylinder start so walls block from both sides.
				// (The C++ BSP guarantees normals face the playable volume;
				// mesh triangles have arbitrary winding.)
				let start_side = normal.dot(&cylinder.start) - plane_dist;
				let (normal, plane_dist) = if start_side < -0.001 {
					(-normal, -plane_dist)
				} else {
					(normal, plane_dist)
				};

				let tri_center = self.tri_centroids[t];
				let tri_r3d = self.tri_radius_3d[t];

				// Sphere early-out (matching BSP Dot test)
				let dot: f32 = normal.dot(&cylinder.move_mid) - plane_dist;
				if dot > cylinder.move_sphere || dot < -cylinder.move_sphere { continue; }

				// Classify start and end (matching BSP state1/state2)
				let state1 = cylinder.get_plane_side(normal, plane_dist, cylinder.start, false, true);
				let state2 = cylinder.get_plane_side(normal, plane_dist, cylinder.end, true, state1 != PolySide::FrontSide);

				let tri = [va, vb, vc];
				if state1 == state2 {
					if state1 == PolySide::Intersect {
						if cylinder.collide_with_triangle(&tri, &normal, tri_center, tri_r3d) {
							collide_normal = normal;
							num_hits += 1;
						}
					}
				} else if state1 == PolySide::FrontSide {
					if cylinder.collide_with_triangle(&tri, &normal, tri_center, tri_r3d) {
						collide_normal = normal;
						num_hits += 1;
					}
				}
			}

			// Handle collision (matching C++ retry logic)
			if old_hit_count != num_hits {
				if retry_collision > 0 && retry_hit_count != num_hits {
					retry_collision -= 1;
					cylinder.handle_collision();
					retry_hit_count = num_hits;
					continue;
				} else {
					if retry_hit_count != num_hits
						|| cylinder.end.x.is_nan()
						|| cylinder.end.y.is_nan()
						|| cylinder.end.z.is_nan()
					{
						cylinder.end = p0 + offset;
					}
					break;
				}
			}

			current_start = current_start + dir_norm * vel_step;
			velocity_left -= vel_step;
		}

		*p1 = cylinder.end - offset;
		(num_hits, collide_normal)
	}

	/// Main collision entry point — adapted from CollideWithWorld() in collision.cpp.
	/// Handles stair stepping followed by iterative cylinder collision.
	pub fn resolve_player_movement(&self, prev_pos: Vector3, pos: &mut Vector3, radius: f32) {
		let dims = Vector3::new(radius, radius, PLAYER_HALF_HEIGHT);
		let mut p0 = prev_pos;
		let mut p1 = *pos;
		let mut offset = Vector3::new(0.0, 0.0, 0.0);
		let mut collision_dims = dims;

		// ---- Stair stepping (matching FLAG_STAIRSTEP path in CollideWithWorld) ----
		let stair_height = (dims.z * 0.5 + 0.01) * 0.5;
		let stair_offset = Vector3::new(0.0, 0.0, -(dims.z - stair_height));
		let mut stair_dims = dims;
		stair_dims.z = stair_height;

		let hit_non_step = self.stair_step(stair_offset, &mut p0, &mut p1, stair_dims);

		if hit_non_step {
			offset.z = 0.0;
		} else {
			offset.z = stair_height;
			collision_dims.z -= stair_height;
			if collision_dims.z < 0.01 { collision_dims.z = 0.01; }
		}

		// ---- Main collision loop (matching the for-loop in CollideWithWorld) ----
		let mut total_hits: u32 = 0;
		for _i in 0..MAX_PHYSICS_ITERATIONS {
			let prev_hits = total_hits;
			let (hits, _normal) = self.collide_cylinder_with_mesh(p0, &mut p1, offset, collision_dims);
			total_hits += hits;
			if prev_hits == total_hits { break; }
		}

		if total_hits >= MAX_PHYSICS_ITERATIONS as u32 {
			p1 = p0;
		}

		*pos = p1;
	}
}

impl HeightProvider for MeshHeightProvider {
	fn ground_height(&self, x: f32, y: f32, current_z: Option<f32>) -> f32 {
		let mut best_below: Option<f32> = None;
		let mut best_above: Option<f32> = None;
		let idx = &self.indices;
		let pos = &self.positions;

		let query_box = AABB {
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
			let i0 = idx[t*3]     as usize;
			let i1 = idx[t*3 + 1] as usize;
			let i2 = idx[t*3 + 2] as usize;
			if i0 >= pos.len() || i1 >= pos.len() || i2 >= pos.len() { continue; }
			let v0 = pos[i0];
			let v1 = pos[i1];
			let v2 = pos[i2];

			if MeshHeightProvider::point_in_tri_2d(x, y, v0, v1, v2) {
				let edge1 = v1 - v0;
				let edge2 = v2 - v0;
				let n = edge1.cross(&edge2);
				if n.z.abs() < 1e-6 { continue; }
				let z = v0.z - (n.x * (x - v0.x) + n.y * (y - v0.y)) / n.z;
				if let Some(curz) = current_z {
					if z > curz + 0.5 { continue; } // skip surfaces way above
					if z <= curz {
						// Surface at or below player center → real ground.
						// Pick the highest one (closest floor below us).
						best_below = Some(best_below.map_or(z, |b: f32| b.max(z)));
					} else {
						// Surface slightly above player center (step-up).
						// Pick the lowest one (smallest step).
						best_above = Some(best_above.map_or(z, |b: f32| b.min(z)));
					}
				} else {
					best_below = Some(best_below.map_or(z, |b: f32| b.max(z)));
				}
			}
		}
		// Prefer surfaces below the player; only fall back to step-ups.
		// This prevents ceilings (which are above curz) from being picked
		// as ground while the player is jumping.
		best_below.or(best_above).unwrap_or(0.0)
	}

	fn ceiling_height(&self, x: f32, y: f32, current_z: f32) -> f32 {
		let mut best: f32 = f32::MAX;
		let idx = &self.indices;
		let pos = &self.positions;

		let query_box = AABB {
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
			let i0 = idx[t*3]     as usize;
			let i1 = idx[t*3 + 1] as usize;
			let i2 = idx[t*3 + 2] as usize;
			if i0 >= pos.len() || i1 >= pos.len() || i2 >= pos.len() { continue; }
			let v0 = pos[i0];
			let v1 = pos[i1];
			let v2 = pos[i2];

			if MeshHeightProvider::point_in_tri_2d(x, y, v0, v1, v2) {
				let edge1 = v1 - v0;
				let edge2 = v2 - v0;
				let n = edge1.cross(&edge2);
				if n.z.abs() < 1e-6 { continue; }
				// Only consider downward-facing surfaces (actual ceilings)
				if n.z > 0.0 { continue; }
				let z = v0.z - (n.x * (x - v0.x) + n.y * (y - v0.y)) / n.z;
				// Surface must be above current_z (head position passed by caller)
				if z > current_z && z < best {
					best = z;
				}
			}
		}
		best
	}
}