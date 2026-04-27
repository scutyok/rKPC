//******************************************************************/
//
// CSwitchRotating — lever/switch that rotates when the player presses E.
//
// On activation the switch rotates by `RotationAngle` degrees around the
// local Z axis over ~0.33 s, then fires its `TriggerTarget`.
//
// CSwitchSlide uses the same SwitchObject but translates instead of rotating
// (re-exports from this file).
//
// DAT properties read:
//   `TriggerTarget`  (String) — linked door / object name.
//   `RotationAngle`  (Float)  — degrees to rotate on activation (default 90°).
//
//******************************************************************/

use cgmath::{Deg, Matrix4, vec3};

use crate::dat::WorldObject;
use crate::object_utils::{dist3, matrix4_to_array, prop_float, prop_string,
    set_draw_group_matrix};
use crate::types::DrawGroup;

/// Radius within which pressing E activates a switch.
pub const INTERACT_RADIUS: f32 = 2.0;

#[derive(Debug, Clone)]
pub struct SwitchObject {
    pub position: [f32; 3],
    pub activated: bool,
    pub target_name: String,
    pub draw_group: usize,
    /// Degrees to rotate on activation (CSwitchSlide: 0 = translation only).
    pub rotate_angle: f32,
    /// Animation progress [0..1].
    pub anim_progress: f32,
}

impl SwitchObject {
    /// Attempt to activate. Returns `true` if this call toggled the switch.
    pub fn activate(&mut self) -> bool {
        if !self.activated {
            self.activated = true;
            return true;
        }
        false
    }

    fn model_matrix(&self) -> [[f32; 4]; 4] {
        let c = self.position;
        let to_origin: Matrix4<f32> =
            Matrix4::from_translation(vec3(-c[0], -c[1], -c[2]));
        let rot: Matrix4<f32> =
            Matrix4::from_angle_z(Deg(self.rotate_angle * self.anim_progress));
        let from_origin: Matrix4<f32> =
            Matrix4::from_translation(vec3(c[0], c[1], c[2]));
        matrix4_to_array(from_origin * rot * to_origin)
    }
}

// ─── DAT construction ────────────────────────────────────────────────────────

pub fn parse_rotating(
    pos: [f32; 3],
    props: Option<&WorldObject>,
    draw_group: usize,
) -> SwitchObject {
    SwitchObject {
        position: pos,
        activated: false,
        target_name: prop_string(props, "TriggerTarget"),
        draw_group,
        rotate_angle: prop_float(props, "RotationAngle", 90.0),
        anim_progress: 0.0,
    }
}

pub fn parse_slide(
    pos: [f32; 3],
    props: Option<&WorldObject>,
    draw_group: usize,
) -> SwitchObject {
    SwitchObject {
        position: pos,
        activated: false,
        target_name: prop_string(props, "TriggerTarget"),
        draw_group,
        rotate_angle: 0.0, // slide switches don't rotate
        anim_progress: 0.0,
    }
}

// ─── Per-frame update ────────────────────────────────────────────────────────

pub fn update(sw: &mut SwitchObject, dt: f32, draw_groups: &mut Vec<DrawGroup>) {
    if sw.activated && sw.anim_progress < 1.0 {
        sw.anim_progress = (sw.anim_progress + dt * 3.0).min(1.0);
        set_draw_group_matrix(draw_groups, sw.draw_group, Some(sw.model_matrix()));
    }
}

// ─── Interaction helper ──────────────────────────────────────────────────────

/// Returns `Some(target_name)` if the player is in range and toggled the switch.
pub fn try_interact(
    sw: &mut SwitchObject,
    player_pos: [f32; 3],
    draw_groups: &mut Vec<DrawGroup>,
) -> Option<String> {
    if dist3(player_pos, sw.position) < INTERACT_RADIUS {
        if sw.activate() {
            set_draw_group_matrix(draw_groups, sw.draw_group, Some(sw.model_matrix()));
            if !sw.target_name.is_empty() {
                return Some(sw.target_name.clone());
            }
        }
    }
    None
}
