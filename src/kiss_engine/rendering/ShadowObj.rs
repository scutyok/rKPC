//! Blob shadow system — Lithtech-style projected shadows beneath models.
//!
//! Shadows project straight down (like the original engine's `dir(0,-1,0)`).
//! Each shadow caster is a creature position; the fragment shader darkens
//! upward-facing world surfaces within a radius beneath each caster.

/// Maximum number of shadow casters sent to the GPU per frame.
pub const MAX_SHADOW_CASTERS: usize = 32;

/// Default blob shadow radius in world units.
pub const SHADOW_RADIUS: f32 = 0.8;

/// GPU-side shadow caster, passed inside the lighting UBO.
#[repr(C, align(16))]
#[derive(Copy, Clone, Debug)]
pub struct GpuShadowCaster {
    /// xyz = position (world space), w = radius
    pub position_radius: [f32; 4],
}

/// Collect shadow caster positions and pack them into GPU structs.
pub fn build_shadow_casters(positions: &[[f32; 3]]) -> (u32, [GpuShadowCaster; MAX_SHADOW_CASTERS]) {
    let count = positions.len().min(MAX_SHADOW_CASTERS);
    let mut casters = [GpuShadowCaster {
        position_radius: [0.0; 4],
    }; MAX_SHADOW_CASTERS];
    for (i, pos) in positions.iter().take(count).enumerate() {
        casters[i] = GpuShadowCaster {
            position_radius: [pos[0], pos[1], pos[2], SHADOW_RADIUS],
        };
    }
    (count as u32, casters)
}
