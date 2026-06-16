pub use crate::util::math::Vector3;
pub use crate::util::geometry::{AABB,Segment};

//******************************************************************/
//
// clip_poly_to_z_range — clips a convex polygon to a Z slab.
// Adapted from ClipPolyToYRange() in collision.cpp (Y-up -> Z-up).
// 
//**   RETURN      *************************************************/
// 
// Returns true if any vertices remain after clipping.
//
//******************************************************************/

pub fn clip_poly_to_z_range(input: &[Vector3], output: &mut Vec<Vector3>, min_z: f32, max_z: f32) -> bool {
	output.clear();
	if input.is_empty() { return false; }

	// Pass 1: clip to min_z
	let mut clip_buffer: Vec<Vector3> = Vec::with_capacity(input.len() + 2);
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

	// Pass 2: clip to max_z
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