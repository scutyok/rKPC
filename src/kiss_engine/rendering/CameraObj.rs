use cgmath::{vec3, InnerSpace};

use crate::types::{Mat4, Vec3};

//******************************************************************/
//
// Camera state for FPS-style controls
//
//******************************************************************/

#[derive(Clone, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: vec3(0.0, 0.0, 12.0),
            yaw: 90.0,
            pitch: 0.0,
        }
    }
}

impl Camera {
    pub fn front(&self) -> Vec3 {
        let yaw_rad = self.yaw.to_radians();
        let pitch_rad = self.pitch.to_radians();
        vec3(
            yaw_rad.cos() * pitch_rad.cos(),
            yaw_rad.sin() * pitch_rad.cos(),
            pitch_rad.sin(),
        )
        .normalize()
    }

    pub fn right(&self) -> Vec3 {
        self.front().cross(vec3(0.0, 0.0, 1.0)).normalize()
    }

    pub fn view_matrix(&self) -> Mat4 {
        self.view_matrix_with_offset(0.0)
    }

    pub fn view_matrix_with_offset(&self, offset_z: f32) -> Mat4 {
        let eye = vec3(
            self.position.x,
            self.position.y,
            self.position.z + offset_z,
        );
        let front = self.front();
        let target = eye + front;
        Mat4::look_at_rh(
            cgmath::Point3::new(eye.x, eye.y, eye.z),
            cgmath::Point3::new(target.x, target.y, target.z),
            vec3(0.0, 0.0, 1.0),
        )
    }
}

//******************************************************************/
//
// Input State Tracking
//
//******************************************************************/

#[derive(Clone, Debug, Default)]
pub struct InputState {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
}
