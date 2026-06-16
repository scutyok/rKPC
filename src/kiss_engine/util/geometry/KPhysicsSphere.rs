pub use crate::util::math::Vector3;

#[derive(Clone, Copy, Debug)]

pub struct PhysicsSphere {
	pub center: Vector3,
	pub radius: f32,
}