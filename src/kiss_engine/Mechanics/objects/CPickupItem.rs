//******************************************************************/
//
// CPickupItem — spinning/bobbing pickup items (health, ammo, weapons, armor, quest items).
//
// In the original KISS Psycho Circus engine, pickup items float and rotate
// slowly to attract the player's attention. This module reproduces that
// behaviour by applying a per-frame Y-axis rotation and a gentle Z-axis
// sine-wave bob to the draw group's model matrix.
//
//******************************************************************/

use cgmath::{Matrix4, Rad, vec3};
use crate::object_utils::{matrix4_to_array, set_draw_group_matrix};
use crate::types::DrawGroup;

/// Rotation speed in radians per second (one full turn every ~4 seconds).
const SPIN_SPEED: f32 = std::f32::consts::TAU / 4.0;

/// Vertical bob amplitude in Vulkan units.
const BOB_AMPLITUDE: f32 = 0.06;

/// Vertical bob frequency in radians per second.
const BOB_FREQUENCY: f32 = 2.5;

#[derive(Debug, Clone)]
pub struct PickupItemObject {
    pub position: [f32; 3],
    pub draw_group: usize,
    /// Accumulated angle for spin (radians).
    pub angle: f32,
}

pub fn parse(pos: [f32; 3], dg: usize) -> PickupItemObject {
    PickupItemObject {
        position: pos,
        draw_group: dg,
        angle: 0.0,
    }
}

pub fn update(item: &mut PickupItemObject, dt: f32, time: f32, draw_groups: &mut Vec<DrawGroup>) {
    item.angle += SPIN_SPEED * dt;
    if item.angle > std::f32::consts::TAU {
        item.angle -= std::f32::consts::TAU;
    }

    let bob = BOB_AMPLITUDE * (BOB_FREQUENCY * time).sin();

    let mat = Matrix4::from_translation(vec3(item.position[0], item.position[1], item.position[2] + bob))
        * Matrix4::from_angle_z(Rad(item.angle))
        * Matrix4::from_translation(vec3(-item.position[0], -item.position[1], -item.position[2]));

    set_draw_group_matrix(draw_groups, item.draw_group, Some(matrix4_to_array(mat)));
}
