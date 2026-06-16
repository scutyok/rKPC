use crate::util::math::Vector3;

#[derive(Clone, Debug)]
pub struct Segment {
	pub origin: Vector3,
	pub direction: Vector3,
}