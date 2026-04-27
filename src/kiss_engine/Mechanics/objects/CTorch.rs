//******************************************************************/
//
// CTorch — decorative torch that emits a flickering point light and an
// animated billboard flame sprite.
//
// Flame textures: REZ/SPRITETEXTURES/FLAMETEST/TORCH1-6.DTX (6 frames @ ~12 fps).
// Light colour  : orange (1.0, 0.55, 0.1).
// Light radius  : 6.0 Vulkan units (~600 Lithtech units).
//
// DAT properties read:
//   `LightIntensity` (Float) — base light intensity (default 1.5).
//
//******************************************************************/

use cgmath::{Matrix4, Rad, vec3};

use crate::dat::WorldObject;
use crate::LightObj::Light;
use crate::object_utils::{matrix4_to_array, prop_float};
use crate::types::DrawGroup;

const FLAME_FRAME_SECS: f32 = 0.08; // ~12 fps animation
const FLAME_FRAMES: usize = 6;
const FLAME_HEIGHT: f32 = 0.35;
const FLAME_Z_OFFSET: f32 = 0.10;

#[derive(Debug, Clone)]
pub struct TorchObject {
    pub position: [f32; 3],
    pub draw_group: usize,
    pub flicker_phase: f32,
    pub base_intensity: f32,
    /// DrawGroup index for the animated flame billboard quad.
    pub flame_draw_group: usize,
    /// Index into `AppData::level_textures` for TORCH1.DTX (frame 0).
    pub flame_base_tex_index: usize,
    pub flame_frame: usize,
    pub flame_frame_timer: f32,
}

// ─── DAT construction ────────────────────────────────────────────────────────

pub fn parse(
    pos: [f32; 3],
    props: Option<&WorldObject>,
    draw_group: usize,
    flame_draw_group: usize,
    flame_base_tex_index: usize,
) -> TorchObject {
    let phase = (pos[0] * 7.3 + pos[1] * 3.7 + pos[2] * 5.1).fract().abs();
    TorchObject {
        position: pos,
        draw_group,
        flicker_phase: phase * std::f32::consts::TAU,
        base_intensity: prop_float(props, "LightIntensity", 1.5),
        flame_draw_group,
        flame_base_tex_index,
        flame_frame: 0,
        flame_frame_timer: 0.0,
    }
}

// ─── Per-frame update ────────────────────────────────────────────────────────

/// Advance flame animation and billboard the quad toward the camera.
pub fn update(
    t: &mut TorchObject,
    dt: f32,
    player_pos: [f32; 3],
    draw_groups: &mut Vec<DrawGroup>,
) {
    // Advance animation frame.
    t.flame_frame_timer += dt;
    if t.flame_frame_timer >= FLAME_FRAME_SECS {
        t.flame_frame_timer -= FLAME_FRAME_SECS;
        t.flame_frame = (t.flame_frame + 1) % FLAME_FRAMES;
    }

    // Billboard: rotate the quad around Z (up) to face the camera in XY.
    let dx = player_pos[0] - t.position[0];
    let dy = player_pos[1] - t.position[1];
    let yaw = Rad(dx.atan2(dy));

    let cx = t.position[0];
    let cy = t.position[1];
    let cz = t.position[2] + FLAME_Z_OFFSET + FLAME_HEIGHT * 0.5;
    let to_origin: Matrix4<f32> = Matrix4::from_translation(vec3(-cx, -cy, -cz));
    let rot: Matrix4<f32> = Matrix4::from_angle_z(yaw);
    let from_origin: Matrix4<f32> = Matrix4::from_translation(vec3(cx, cy, cz));

    if let Some(dg) = draw_groups.get_mut(t.flame_draw_group) {
        dg.texture_index = t.flame_base_tex_index + t.flame_frame;
        dg.model_matrix = Some(matrix4_to_array(from_origin * rot * to_origin));
    }
}

// ─── Dynamic light ──────────────────────────────────────────────────────────

/// Build the per-frame flickering point light for this torch.
/// `time` is total elapsed seconds used for the sine-wave flicker.
pub fn dynamic_light(t: &TorchObject, time: f32) -> Light {
    let flicker = (0.85
        + 0.10 * (time * 7.3 + t.flicker_phase).sin()
        + 0.05 * (time * 17.1 + t.flicker_phase * 2.3).sin())
        .max(0.5);
    Light {
        position: t.position,
        radius: 6.0,
        color: [1.0, 0.55, 0.1],
        intensity: t.base_intensity * flicker,
    }
}
