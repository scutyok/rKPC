//! Lighting UBO builder — assembles the per-frame `LightingUBO` from world
//! lights, shadow casters, fog settings, and ambient intensity.

use crate::LightObj::Light;
use crate::ShadowObj;
use crate::types::{GpuLight, LightingUBO, MAX_LIGHTS};

/// Build a complete `LightingUBO` ready for GPU upload.
///
/// Combines point lights, shadow casters, fog parameters, and ambient colour
/// into a single UBO struct that matches the GLSL `LightingData` layout.
pub fn build_light_ubo(
    world_lights: &[Light],
    shadow_positions: &[[f32; 3]],
    fog_color: [f32; 3],
    fog_near: f32,
    fog_far: f32,
    fog_enabled: bool,
    sky_fog_far: f32,
) -> LightingUBO {
    let mut ubo = LightingUBO::default();
    let count = world_lights.len().min(MAX_LIGHTS);
    ubo.light_count = count as u32;
    ubo.ambient = [0.15, 0.15, 0.15, 0.0];
    ubo.fog_color = [fog_color[0], fog_color[1], fog_color[2], 0.0];
    ubo.fog_params = [fog_near, fog_far, if fog_enabled { 1.0 } else { 0.0 }, sky_fog_far];

    for (i, l) in world_lights.iter().take(count).enumerate() {
        let r_sq = l.radius * l.radius;
        let inv_r = if l.radius > 0.0 { 1.0 / l.radius } else { 0.0 };
        ubo.lights[i] = GpuLight {
            position_radius_sq: [l.position[0], l.position[1], l.position[2], r_sq],
            color_intensity: [
                l.color[0] * l.intensity,
                l.color[1] * l.intensity,
                l.color[2] * l.intensity,
                inv_r,
            ],
        };
    }

    // Populate shadow casters (creature blob shadows)
    let (shadow_count, casters) = ShadowObj::build_shadow_casters(shadow_positions);
    ubo.shadow_count = shadow_count;
    ubo.shadow_casters = casters;

    ubo
}
