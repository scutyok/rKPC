pub use crate::util::math::Vector3;
pub use crate::util::geometry::{AABB,Segment};
use super::HeightProvider;

//******************************************************************/
//
// PlayerMode
//
//******************************************************************/

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerMode { Flying, Walk }

//******************************************************************/
//
// resolve_player_collision — vertical ground correction
//
//******************************************************************/

pub fn resolve_player_collision(pos: &mut Vector3, height_provider: &dyn HeightProvider, player_radius: f32, step_height: f32) {
	let ground_z = height_provider.ground_height(pos.x, pos.y, Some(pos.z));
	let min_z = ground_z + player_radius;
	if pos.z < min_z {
		let diff = min_z - pos.z;
		// Only snap upward if the correction is within a step height.
		// Large diffs mean ground_height found an upper surface — don't teleport.
		if diff <= step_height {
			pos.z = min_z;
		}
	}
}

