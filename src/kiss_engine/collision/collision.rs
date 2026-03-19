use cgmath::{Vector3, InnerSpace};

// ------------------------------------------------------------------ //
// Constants matching collision.cpp
// ------------------------------------------------------------------ //

const MAX_PHYSICS_ITERATIONS: usize = 40;
#[allow(dead_code)]
const MAX_INTERSECT_PUSHBACK_ITERATIONS: usize = 50;
#[allow(dead_code)]
const EXTRA_PENETRATION_ADD: f32 = 0.05;

// Player cylinder dimensions (Z-up: height along Z, radius in XY)
const PLAYER_HALF_HEIGHT: f32 = 0.5;
const STAIR_ALLOW_UP_LIMIT: f32 = 0.7071; // cos(45°) — steepest walkable surface


// ------------------------------------------------------------------ //
// PolySide — matches the C++ FrontSide / BackSide / Intersect enum
// ------------------------------------------------------------------ //

#[derive(Clone, Copy, PartialEq, Debug)]
enum PolySide { FrontSide, BackSide, Intersect }


// ------------------------------------------------------------------ //
// Aabb — axis-aligned bounding box (kept from original, matching C++ AABB)
// ------------------------------------------------------------------ //

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

	/// Line-segment / AABB intersection using the separating axis theorem.
	/// Matches AABBIntersectsLineSegment() in collision.cpp.
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


// ------------------------------------------------------------------ //
// PhysicsSphere — bounding sphere for quick culling
// ------------------------------------------------------------------ //

#[derive(Clone, Copy, Debug)]
pub struct PhysicsSphere {
	pub center: Vector3<f32>,
	pub radius: f32,
}


// ------------------------------------------------------------------ //
// Segment — matches SBlockerSeg from world_blocker_math
// ------------------------------------------------------------------ //

#[derive(Clone, Debug)]
struct Segment {
	origin: Vector3<f32>,
	direction: Vector3<f32>,
}


// ------------------------------------------------------------------ //
// dist_sqr_seg_seg — squared distance between two line segments.
// Matches DistSqrSegSeg() from world_blocker_math.cpp.
// Returns the squared distance; fills out_p0 / out_p1 with the
// parameter values [0,1] at the closest points on each segment.
// ------------------------------------------------------------------ //

fn dist_sqr_seg_seg(s0: &Segment, s1: &Segment, out_p0: &mut f32, out_p1: &mut f32) -> f32 {
	let d1 = s0.direction;
	let d2 = s1.direction;
	let r  = s0.origin - s1.origin;

	let a = d1.dot(d1);     // |D0|²
	let e = d2.dot(d2);     // |D1|²
	let f = d2.dot(r);

	let (s, t);

	if a <= 1e-6 && e <= 1e-6 {
		// Both segments degenerate to points
		s = 0.0; t = 0.0;
	} else if a <= 1e-6 {
		// First segment degenerates
		s = 0.0;
		t = (f / e).clamp(0.0, 1.0);
	} else {
		let c = d1.dot(r);
		if e <= 1e-6 {
			// Second segment degenerates
			t = 0.0;
			s = (-c / a).clamp(0.0, 1.0);
		} else {
			// General non-degenerate case
			let b = d1.dot(d2);
			let denom = a * e - b * b;

			let s_n = if denom.abs() > 1e-6 {
				((b * f - c * e) / denom).clamp(0.0, 1.0)
			} else {
				0.0
			};

			// Compute t from s
			let t_n = (b * s_n + f) / e;

			if t_n < 0.0 {
				t = 0.0;
				s = (-c / a).clamp(0.0, 1.0);
			} else if t_n > 1.0 {
				t = 1.0;
				s = ((b - c) / a).clamp(0.0, 1.0);
			} else {
				s = s_n;
				t = t_n;
			}
		}
	}

	*out_p0 = s;
	*out_p1 = t;

	let diff = r + d1 * s - d2 * t;
	diff.dot(diff)
}


// ------------------------------------------------------------------ //
// clip_poly_to_z_range — clips a convex polygon to a Z slab.
// Adapted from ClipPolyToYRange() in collision.cpp (Y-up → Z-up).
// Returns true if any vertices remain after clipping.
// ------------------------------------------------------------------ //

fn clip_poly_to_z_range(input: &[Vector3<f32>], output: &mut Vec<Vector3<f32>>, min_z: f32, max_z: f32) -> bool {
	output.clear();
	if input.is_empty() { return false; }

	// --- Pass 1: clip to min_z ---
	let mut clip_buffer: Vec<Vector3<f32>> = Vec::with_capacity(input.len() + 2);
	{
		let mut prev = *input.last().unwrap();
		let mut prev_dist = prev.z - min_z;
		for &cur in input.iter() {
			let cur_dist = cur.z - min_z;
			if cur_dist * prev_dist < 0.0 {
				let interp = prev_dist / (prev_dist - cur_dist);
				clip_buffer.push(prev + (cur - prev) * interp);
			}
			if cur_dist >= -0.0001 {
				clip_buffer.push(cur);
			}
			prev = cur;
			prev_dist = cur_dist;
		}
	}
	if clip_buffer.is_empty() { return false; }

	// --- Pass 2: clip to max_z ---
	{
		let mut prev = *clip_buffer.last().unwrap();
		let mut prev_dist = prev.z - max_z;
		for &cur in clip_buffer.iter() {
			let cur_dist = cur.z - max_z;
			if cur_dist * prev_dist < 0.0 {
				let interp = prev_dist / (prev_dist - cur_dist);
				output.push(prev + (cur - prev) * interp);
			}
			if cur_dist <= 0.0001 {
				output.push(cur);
			}
			prev = cur;
			prev_dist = cur_dist;
		}
	}

	!output.is_empty()
}


// ------------------------------------------------------------------ //
// MovingCylinder — adapted from CMovingCylinder in collision.cpp.
// Z-up: height along Z axis, radius in XY plane.
// ------------------------------------------------------------------ //

struct MovingCylinder {
	// --- Provided members ---
	start:   Vector3<f32>,
	end:     Vector3<f32>,
	real_end: Vector3<f32>,
	radius:  f32,
	height:  f32,             // half total cylinder height

	// --- Calculated by recalc() ---
	sphere:          f32,     // bounding sphere radius around cylinder
	movement:        Vector3<f32>,
	movement_dir:    Vector3<f32>,
	velocity:        f32,
	move_mid:        Vector3<f32>,
	move_sphere:     f32,
	radius_sqr:      f32,
	sphere_sqr:      f32,
	move_sphere_sqr: f32,
	velocity_sqr:    f32,
	move_top:        f32,     // max Z extent of swept cylinder
	move_bottom:     f32,     // min Z extent of swept cylinder

	// --- Collision information (populated by collide_with_triangle) ---
	dist_to_plane:   f32,
	plane_intrusion: f32,
	closest_pt:      Vector3<f32>,
	closest_dir:     Vector3<f32>,
	closest_dist:    f32,
}

impl MovingCylinder {
	fn new(start: Vector3<f32>, end: Vector3<f32>, radius: f32, height: f32) -> Self {
		let mut c = MovingCylinder {
			start, end, real_end: end, radius, height,
			sphere: 0.0, movement: Vector3::new(0.0,0.0,0.0),
			movement_dir: Vector3::new(0.0,0.0,0.0), velocity: 0.0,
			move_mid: Vector3::new(0.0,0.0,0.0), move_sphere: 0.0,
			radius_sqr: 0.0, sphere_sqr: 0.0, move_sphere_sqr: 0.0,
			velocity_sqr: 0.0, move_top: 0.0, move_bottom: 0.0,
			dist_to_plane: 0.0, plane_intrusion: 0.0,
			closest_pt: Vector3::new(0.0,0.0,0.0),
			closest_dir: Vector3::new(0.0,0.0,0.0), closest_dist: 0.0,
		};
		c.recalc();
		c
	}

	/// Matches CMovingCylinder::Recalc()
	fn recalc(&mut self) {
		self.radius_sqr = self.radius * self.radius;
		self.sphere_sqr = self.height * self.height + self.radius_sqr;
		self.sphere = self.sphere_sqr.sqrt();

		self.movement = self.end - self.start;
		self.velocity_sqr = self.movement.dot(self.movement);
		self.velocity = self.velocity_sqr.sqrt();
		if self.velocity > 0.0 {
			self.movement_dir = self.movement * (1.0 / self.velocity);
		} else {
			self.movement_dir = Vector3::new(0.0, 0.0, 0.0);
		}
		self.move_mid = (self.start + self.end) * 0.5;
		self.move_sphere = self.sphere + self.velocity * 0.5;
		self.move_sphere_sqr = self.move_sphere * self.move_sphere;

		// Z-up: top and bottom of the swept volume
		self.move_top    = self.start.z.max(self.end.z) + self.height;
		self.move_bottom = self.start.z.min(self.end.z) - self.height;

		self.closest_dist = 0.0;
	}

	/// Matches CMovingCylinder::GetPlaneSide()
	/// Z-up adaptation: Y vertical → Z vertical.
	fn get_plane_side(&mut self, normal: Vector3<f32>, plane_dist: f32,
					  center: Vector3<f32>, is_end: bool, optimize: bool) -> PolySide
	{
		let d = normal.dot(center) - plane_dist;
		if is_end { self.dist_to_plane = d; }

		if optimize {
			if d >= self.sphere  { return PolySide::FrontSide; }
			if d <= -self.sphere { return PolySide::BackSide; }

			// Vertical plane (normal.z == 0): only radius matters
			if normal.z == 0.0 {
				if d >= self.radius  { return PolySide::FrontSide; }
				if d <= -self.radius { return PolySide::BackSide; }
				if is_end { self.plane_intrusion = self.radius - d; }
				return PolySide::Intersect;
			}
			// Horizontal plane (normal.x == 0 && normal.y == 0): only height matters
			if normal.x == 0.0 && normal.y == 0.0 {
				if d >= self.height  { return PolySide::FrontSide; }
				if d <= -self.height { return PolySide::BackSide; }
				if is_end { self.plane_intrusion = self.height - d; }
				return PolySide::Intersect;
			}
		}

		// General case
		let plane_circle = (normal.x * normal.x + normal.y * normal.y).sqrt().min(1.0);
		let plane_radius = plane_circle * self.radius;
		let plane_height = (1.0 - plane_circle * plane_circle).max(0.0).sqrt() * self.height;
		let proj_max = plane_radius + plane_height;

		if d >= proj_max {
			PolySide::FrontSide
		} else if d <= -proj_max {
			if !optimize && is_end {
				self.plane_intrusion = proj_max * 2.0 - d;
			}
			PolySide::BackSide
		} else {
			if is_end { self.plane_intrusion = proj_max - d; }
			PolySide::Intersect
		}
	}

	/// Matches CMovingCylinder::CollideWith() adapted for a triangle.
	/// Z-up: Y vertical references → Z vertical.
	fn collide_with_triangle(&mut self, verts: &[Vector3<f32>; 3], normal: &Vector3<f32>,
							 tri_center: Vector3<f32>, tri_radius: f32) -> bool
	{
		// Back-facing early out
		if self.movement_dir.dot(*normal) > 0.01 { return false; }

		// Sphere–sphere culling
		let poly_r = tri_radius + self.move_sphere;
		let cd = tri_center - self.move_mid;
		if cd.dot(cd) > poly_r * poly_r { return false; }

		// ---- Top / bottom cap collision (Z-up) ----
		if (self.start.z - self.end.z).abs() > 0.001
			&& normal.z.abs() > 0.001
			&& self.plane_intrusion > self.closest_dist
		{
			let is_top = normal.z < 0.0;
			let h_off = if is_top { self.height } else { -self.height };
			let cap_start = self.start.z + h_off;
			let cap_end   = self.end.z   + h_off;

			let mut clipped = Vec::new();
			if clip_poly_to_z_range(verts, &mut clipped, cap_start.min(cap_end), cap_start.max(cap_end)) {
				// Skew polygon so we can test against a vertical cylinder
				let dz = self.end.z - self.start.z;
				if dz.abs() > 0.001 {
					let skew = Vector3::new(
						(self.end.x - self.start.x) / dz,
						(self.end.y - self.start.y) / dz,
						0.0,
					);
					for v in clipped.iter_mut() {
						let off = v.z - self.start.z;
						v.x += skew.x * off;
						v.y += skew.y * off;
					}
				}

				let cap_seg = Segment {
					origin: Vector3::new(self.start.x, self.start.y, self.start.z + h_off),
					direction: Vector3::new(0.0, 0.0, self.end.z - self.start.z),
				};

				let mut intersect = false;
				let mut inside = true;
				let n_verts = clipped.len();

				for i in 0..n_verts {
					if intersect { break; }
					let prev = if i == 0 { n_verts - 1 } else { i - 1 };
					let edge = Segment {
						origin: clipped[prev],
						direction: clipped[i] - clipped[prev],
					};
					let mut _p0 = 0.0;
					let mut _p1 = 0.0;
					if dist_sqr_seg_seg(&cap_seg, &edge, &mut _p0, &mut _p1) < self.radius_sqr {
						intersect = true;
					}
					if inside {
						let w = (clipped[prev].x - self.start.x) * edge.direction.y
							  - (clipped[prev].y - self.start.y) * edge.direction.x;
						let w = if is_top { -w } else { w };
						inside = w >= 0.0;
					}
				}

				if intersect || inside {
					let z_min = clipped.iter().map(|v| v.z).fold(f32::MAX, f32::min);
					let z_max = clipped.iter().map(|v| v.z).fold(f32::MIN, f32::max);
					self.closest_pt = self.end;

					let (cd, cdir);
					if is_top {
						if z_min - 0.001 > cap_start {
							cd = cap_end - z_min;
							cdir = Vector3::new(0.0, 0.0, 1.0);
						} else {
							cd = self.plane_intrusion;
							cdir = Vector3::new(-normal.x, -normal.y, -normal.z);
						}
					} else {
						if z_max + 0.001 < cap_start {
							cd = z_max - cap_end;
							cdir = Vector3::new(0.0, 0.0, -1.0);
						} else {
							cd = self.plane_intrusion;
							cdir = Vector3::new(-normal.x, -normal.y, -normal.z);
						}
					}

					if cd <= self.closest_dist { return false; }
					self.closest_dist = cd;
					self.closest_dir  = cdir;
					return true;
				}
			}
		}

		// ---- Middle-section (cylinder body) collision (Z-up) ----
		let mid_min_z = self.start.z.max(self.end.z) - self.height;
		let mid_max_z = self.start.z.min(self.end.z) + self.height;

		let mut clipped = Vec::new();
		if !clip_poly_to_z_range(verts, &mut clipped, mid_min_z, mid_max_z) {
			return false;
		}

		// Project movement to XY plane
		let move_seg = Segment {
			origin: Vector3::new(self.start.x, self.start.y, 0.0),
			direction: Vector3::new(self.end.x - self.start.x, self.end.y - self.start.y, 0.0),
		};

		let mut best_dist = f32::MAX;
		let mut best_p0 = 1.0f32;
		let mut best_p1 = 0.0f32;
		let mut best_seg = Segment { origin: Vector3::new(0.0,0.0,0.0), direction: Vector3::new(0.0,0.0,0.0) };

		let nv = clipped.len();
		for i in 0..nv {
			let prev = if i == 0 { nv - 1 } else { i - 1 };
			let edge_seg = Segment {
				origin: Vector3::new(clipped[prev].x, clipped[prev].y, 0.0),
				direction: Vector3::new(clipped[i].x - clipped[prev].x, clipped[i].y - clipped[prev].y, 0.0),
			};
			let mut p0 = 0.0;
			let mut p1 = 0.0;
			let sd = dist_sqr_seg_seg(&move_seg, &edge_seg, &mut p0, &mut p1);

			if sd < best_dist {
				let pt_on_seg = edge_seg.origin + edge_seg.direction * p1;
				let off = pt_on_seg - Vector3::new(self.start.x, self.start.y, 0.0);
				if off.dot(move_seg.direction) > 0.0 || p0 < 0.01 {
					best_seg  = edge_seg;
					best_p0   = p0;
					best_p1   = p1;
					best_dist = sd;
				}
			}
		}

		if best_dist > self.radius_sqr { return false; }

		let intrusion = self.radius - best_dist.sqrt();
		if intrusion <= self.closest_dist { return false; }

		// If already intersecting at movement start, back up (matching C++)
		if best_p0 < 1.0 && intrusion >= self.radius {
			let backed = Segment {
				origin: Vector3::new(self.start.x, self.start.y, 0.0),
				direction: Vector3::new(0.0, 0.0, 0.0),
			};
			best_p0 = 0.0;
			dist_sqr_seg_seg(&backed, &best_seg, &mut best_p0, &mut best_p1);
			best_p0 = 0.0; // force to start
		}

		let poly_pt = best_seg.origin + best_seg.direction * best_p1;
		let move_pt = Vector3::new(self.start.x, self.start.y, 0.0) + move_seg.direction * best_p0;
		let mut dir = poly_pt - move_pt;
		let dir_mag = (dir.x * dir.x + dir.y * dir.y).sqrt();
		if dir_mag > 1e-6 { dir.x /= dir_mag; dir.y /= dir_mag; }
		dir.z = 0.0;

		self.closest_pt = Vector3::new(
			move_pt.x,
			move_pt.y,
			self.start.z + (self.end.z - self.start.z) * best_p0,
		);
		self.closest_dir  = dir;
		self.closest_dist = intrusion;
		true
	}

	/// Wall-slide collision response.
	fn handle_collision(&mut self) {
		// 1) Push the endpoint just out of penetration along the collision normal
		let push_out = self.closest_dir * -(self.closest_dist + 0.001);
		let resolved = self.closest_pt + push_out;

		// 2) Compute remaining intended movement from the collision point
		let remaining = self.real_end - resolved;

		// 3) Wall normal (the inward direction the wall pushes us)
		let wall_n = self.closest_dir;
		let n_len = wall_n.magnitude();

		// 4) Slide: subtract the component of remaining movement that goes into the wall
		let slid = if n_len > 1e-6 {
			let n = wall_n * (1.0 / n_len);
			let into_wall = remaining.dot(n);
			if into_wall > 0.0 {
				remaining - n * into_wall
			} else {
				remaining
			}
		} else {
			remaining
		};

		self.end = resolved + slid;
		self.recalc();
	}
}


// ------------------------------------------------------------------ //
// HeightProvider trait & FlatGround
// ------------------------------------------------------------------ //

pub trait HeightProvider: Send + Sync {
	fn ground_height(&self, x: f32, y: f32, current_z: Option<f32>) -> f32;
}

pub struct FlatGround;
impl HeightProvider for FlatGround {
	fn ground_height(&self, _x: f32, _y: f32, _current_z: Option<f32>) -> f32 { 0.0 }
}


// ------------------------------------------------------------------ //
// PlayerMode
// ------------------------------------------------------------------ //

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerMode { Flying, Walk }


// ------------------------------------------------------------------ //
// resolve_player_collision — vertical ground correction
// ------------------------------------------------------------------ //

pub fn resolve_player_collision(pos: &mut Vector3<f32>, height_provider: &dyn HeightProvider, player_radius: f32, _step_height: f32) {
	let ground_z = height_provider.ground_height(pos.x, pos.y, Some(pos.z));
	let min_z = ground_z + player_radius;
	if pos.z < min_z {
		let diff = min_z - pos.z;
		if diff <= 0.5 {
			pos.z = min_z;
		} else {
			pos.z += 0.3;
		}
	}
}


// ------------------------------------------------------------------ //
// MeshHeightProvider
// ------------------------------------------------------------------ //

#[derive(Clone)]
pub struct MeshHeightProvider {
	pub positions:      Vec<Vector3<f32>>,
	pub indices:        Vec<u32>,
	pub tri_centroids:  Vec<Vector3<f32>>,
	pub tri_radius_xy:  Vec<f32>,
	pub tri_radius_3d:  Vec<f32>,
	pub bvh:            Option<bvh::Bvh>,
}

impl MeshHeightProvider {
	pub fn new(positions: Vec<Vector3<f32>>, indices: Vec<u32>) -> Self {
		let mut tri_centroids  = Vec::new();
		let mut tri_radius_xy  = Vec::new();
		let mut tri_radius_3d  = Vec::new();
		let tri_count = indices.len() / 3;

		for t in 0..tri_count {
			let i0 = indices[t*3]     as usize;
			let i1 = indices[t*3 + 1] as usize;
			let i2 = indices[t*3 + 2] as usize;
			if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
				tri_centroids.push(Vector3::new(0.0, 0.0, 0.0));
				tri_radius_xy.push(0.0);
				tri_radius_3d.push(0.0);
				continue;
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
			tri_centroids.push(centroid);
			tri_radius_xy.push(rxy);
			tri_radius_3d.push(r3d);
		}

		let bvh = if tri_count > 0 {
			Some(bvh::Bvh::new(&positions, &indices))
		} else { None };

		Self { positions, indices, tri_centroids, tri_radius_xy, tri_radius_3d, bvh }
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

	/// Setup swept AABB — adapted from SetupAxisAlignedBox() in collision.cpp.
	fn setup_axis_aligned_box(
		p0: Vector3<f32>, p1: Vector3<f32>,
		dims: Vector3<f32>, offset: Vector3<f32>,
	) -> (PhysicsSphere, Aabb) {
		let vs = p0 + offset;
		let ve = p1 + offset;

		let b = Aabb {
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

	/// Cylinder–triangle intersection test for stair stepping.
	/// Adapted from StairStep_CylinderPolyIntersect() in collision.cpp.
	/// Z-up: base is the bottom of the cylinder, height is full height.
	fn stair_step_cylinder_tri_intersect(
		base: Vector3<f32>, cyl_height: f32, cyl_radius: f32,
		verts: &[Vector3<f32>; 3], normal: &Vector3<f32>,
		tri_center: Vector3<f32>, tri_radius_3d: f32,
		result_max_z: &mut f32,
	) -> bool {
		let adj_r = if cyl_radius > 0.1 { cyl_radius - 0.1 } else { cyl_radius };
		let half_h = cyl_height * 0.5;
		let sphere_r = (half_h * half_h + adj_r * adj_r).sqrt();
		let cyl_center = Vector3::new(base.x, base.y, base.z + half_h);
		let cd = cyl_center - tri_center;
		let total_r = sphere_r + tri_radius_3d;
		if cd.dot(cd) > total_r * total_r { return false; }

		if normal.z.abs() < 0.001 { return false; }
		let plane_dist = normal.dot(verts[0]);
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

	/// Single-segment stair step — adapted from StairStep_Segment() in collision.cpp.
	fn stair_step_segment(
		&self, offset: Vector3<f32>,
		p0: &mut Vector3<f32>, p1: &mut Vector3<f32>,
		dims: Vector3<f32>,
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
			let n = edge1.cross(edge2);
			let nmag = n.magnitude();
			if nmag < 1e-6 { continue; }
			let normal = n / nmag;

			// Must be floor-like (nearly horizontal). Handle both winding
			// orders by flipping so normal points up.
			let normal = if normal.z < 0.0 { -normal } else { normal };
			if normal.z <= 0.001 { continue; }

			// Quick sphere cull
			let d = normal.dot(ws.center) - normal.dot(va);
			if d.abs() > ws.radius { continue; }

			let tri = [va, vb, vc];
			let tri_center = self.tri_centroids[t];
			let tri_r3d = self.tri_radius_3d[t];

			let mut max_z = 0.0f32;
			if !Self::stair_step_cylinder_tri_intersect(cb, cyl_height, cyl_radius, &tri, &normal, tri_center, tri_r3d, &mut max_z) {
				continue;
			}

			let max_push = max_z - bx.min.z;

			if max_push > 0.0 && normal.z > STAIR_ALLOW_UP_LIMIT {
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
		&self, offset: Vector3<f32>,
		p0: &mut Vector3<f32>, p1: &mut Vector3<f32>,
		dims: Vector3<f32>,
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

	/// Cylinder–mesh collision — adapted from CollideCylinderWithTree() in collision.cpp.
	/// Uses BVH instead of BSP; otherwise follows the same segmented traversal.
	fn collide_cylinder_with_mesh(
		&self, p0: Vector3<f32>, p1: &mut Vector3<f32>,
		offset: Vector3<f32>, dims: Vector3<f32>,
	) -> (u32, Vector3<f32>) {
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
			let move_aabb = Aabb {
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
				let n = edge1.cross(edge2);
				let nmag = n.magnitude();
				if nmag < 1e-6 { continue; }
				let normal = n / nmag;
				let plane_dist = normal.dot(va);

				// Make triangles double-sided: flip normal to face toward
				// the cylinder start so walls block from both sides.
				// (The C++ BSP guarantees normals face the playable volume;
				// mesh triangles have arbitrary winding.)
				let start_side = normal.dot(cylinder.start) - plane_dist;
				let (normal, plane_dist) = if start_side < -0.001 {
					(-normal, -plane_dist)
				} else {
					(normal, plane_dist)
				};

				let tri_center = self.tri_centroids[t];
				let tri_r3d = self.tri_radius_3d[t];

				// Sphere early-out (matching BSP Dot test)
				let dot = normal.dot(cylinder.move_mid) - plane_dist;
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
	pub fn resolve_player_movement(&self, prev_pos: Vector3<f32>, pos: &mut Vector3<f32>, radius: f32) {
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


// ------------------------------------------------------------------ //
// closest_point_on_triangle — barycentric projection (kept from original)
// ------------------------------------------------------------------ //

#[allow(dead_code)]
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


// ------------------------------------------------------------------ //
// HeightProvider for MeshHeightProvider (ground ray-cast — kept from original)
// ------------------------------------------------------------------ //

impl HeightProvider for MeshHeightProvider {
	fn ground_height(&self, x: f32, y: f32, current_z: Option<f32>) -> f32 {
		let mut best: Option<f32> = None;
		let idx = &self.indices;
		let pos = &self.positions;

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
				let n = edge1.cross(edge2);
				if n.z.abs() < 1e-6 { continue; }
				let z = v0.z - (n.x * (x - v0.x) + n.y * (y - v0.y)) / n.z;
				if let Some(curz) = current_z {
					if z > curz + 0.6 { continue; }
				}
				best = Some(match best {
					None => z,
					Some(prev_best) => {
						if let Some(curz) = current_z {
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


