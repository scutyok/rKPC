pub use crate::util::math::Vector3;
pub use crate::util::geometry::*;

use super::KClipPolyToZRange::clip_poly_to_z_range;

//******************************************************************/
//
// KMovingCylinder
// Z-up: height along Z axis, radius in XY plane.
//
//******************************************************************/

pub struct MovingCylinder {
	// Provided members
	pub start:   Vector3,
	pub end:     Vector3,
	pub real_end: Vector3,
	pub radius:  f32,
	pub height:  f32,             // half total cylinder height

	// Calculated by recalc()
	pub sphere:          f32,     // bounding sphere radius around cylinder
	pub movement:        Vector3,
	pub movement_dir:    Vector3,
	pub velocity:        f32,
	pub move_mid:        Vector3,
	pub move_sphere:     f32,
	pub radius_sqr:      f32,
	pub sphere_sqr:      f32,
	pub move_sphere_sqr: f32,
	pub velocity_sqr:    f32,
	pub move_top:        f32,     // max Z extent of swept cylinder
	pub move_bottom:     f32,     // min Z extent of swept cylinder

	// Collision information (populated by collide_with_triangle)
	pub dist_to_plane:   f32,
	pub plane_intrusion: f32,
	pub closest_pt:      Vector3,
	pub closest_dir:     Vector3,
	pub closest_dist:    f32,
}

impl MovingCylinder {
	pub fn new(start: Vector3, end: Vector3, radius: f32, height: f32) -> Self {
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

	// Matches CMovingCylinder::Recalc() from lithtech source code
	pub fn recalc(&mut self) {
		self.radius_sqr = self.radius * self.radius;
		self.sphere_sqr = self.height * self.height + self.radius_sqr;
		self.sphere = self.sphere_sqr.sqrt();

		self.movement = self.end - self.start;
		self.velocity_sqr = self.movement.dot(&self.movement);
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

	// Matches CMovingCylinder::GetPlaneSide()
	// Z-up adaptation: Y vertical -> Z vertical.
	pub fn get_plane_side(&mut self, normal: Vector3, plane_dist: f32,
					  center: Vector3, is_end: bool, optimize: bool) -> PolySide
	{
		let d = normal.dot(&center) - plane_dist;
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

	// Matches CMovingCylinder::CollideWith() adapted for a triangle.
	// Z-up: Y vertical references -> Z vertical.
	pub fn collide_with_triangle(&mut self, verts: &[Vector3; 3], normal: &Vector3,
							 tri_center: Vector3, tri_radius: f32) -> bool
	{
		// Back-facing early out
		if self.movement_dir.dot(&normal) > 0.01 { return false; }

		// Sphere–sphere culling
		let poly_r = tri_radius + self.move_sphere;
		let cd = tri_center - self.move_mid;
		if cd.dot(&cd) > poly_r * poly_r { return false; }

		// Top / bottom cap collision (Z-up)
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

		// Middle-section (cylinder body) collision (Z-up)
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
				if off.dot(&move_seg.direction) > 0.0 || p0 < 0.01 {
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

	// Wall-slide collision response.
	pub fn handle_collision(&mut self) {
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
			let into_wall = remaining.dot(&n);
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