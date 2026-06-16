pub use crate::util::math::Vector3;
pub use crate::util::geometry::{AABB,Segment};

//******************************************************************/
//
// HeightProvider trait & FlatGround
//
//******************************************************************/

pub trait HeightProvider: Send + Sync {
	fn ground_height(&self, x: f32, y: f32, current_z: Option<f32>) -> f32;
	// Returns the Z of the lowest surface *above* `current_z`.
	// If nothing is found, returns `f32::MAX`.
	fn ceiling_height(&self, x: f32, y: f32, current_z: f32) -> f32;
}

pub struct FlatGround;

impl HeightProvider for FlatGround {
	fn ground_height(&self, _x: f32, _y: f32, _current_z: Option<f32>) -> f32 { 0.0 }
	fn ceiling_height(&self, _x: f32, _y: f32, _current_z: f32) -> f32 { f32::MAX }
}