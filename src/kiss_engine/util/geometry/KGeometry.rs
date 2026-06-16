
pub use crate::util::math::Vector3;
pub use crate::util::geometry::{AABB,Segment};

//******************************************************************/
//
// dist_sqr_seg_seg — squared distance between two line segments.
// Matches DistSqrSegSeg() from world_blocker_math.cpp.
//
//**   RETURN      *************************************************/
//
// Returns the squared distance; fills out_p0 / out_p1 with the
// parameter values [0,1] at the closest points on each segment.
//
//******************************************************************/

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PolySide {
    FrontSide,
    BackSide,
    Intersect,
}

pub fn dist_sqr_seg_seg(s0: &Segment, s1: &Segment, out_p0: &mut f32, out_p1: &mut f32) -> f32 {
	let d1 = s0.direction;
	let d2 = s1.direction;
	let r  = s0.origin - s1.origin;

	let a = d1.dot(&d1);     // |D0|^2
	let e = d2.dot(&d2);     // |D1|^2
	let f = d2.dot(&r);

	let (s, t);

	if a <= 1e-6 && e <= 1e-6 {
		// Both segments degenerate to points
		s = 0.0; t = 0.0;
	} else if a <= 1e-6 {
		// First segment degenerates
		s = 0.0;
		t = (f / e).clamp(0.0, 1.0);
	} else {
		let c = d1.dot(&r);
		if e <= 1e-6 {
			// Second segment degenerates
			t = 0.0;
			s = (-c / a).clamp(0.0, 1.0);
		} else {
			// General non-degenerate case
			let b = d1.dot(&d2);
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
	diff.dot(&diff)
}

//******************************************************************/
//
// closest_point_on_triangle — barycentric projection
//
//******************************************************************/

#[allow(dead_code)]
pub fn closest_point_on_triangle(p: Vector3, a: Vector3, b: Vector3, c: Vector3) -> Vector3 {
	let ab = b - a;
	let ac = c - a;
	let ap = p - a;

	let d1 = ab.dot(&ap);
	let d2 = ac.dot(&ap);
	if d1 <= 0.0 && d2 <= 0.0 { return a; }

	let bp = p - b;
	let d3 = ab.dot(&bp);
	let d4 = ac.dot(&bp);
	if d3 >= 0.0 && d4 <= d3 { return b; }

	let vc = d1 * d4 - d3 * d2;
	if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
		let v = d1 / (d1 - d3);
		return a + ab * v;
	}

	let cp = p - c;
	let d5 = ab.dot(&cp);
	let d6 = ac.dot(&cp);
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