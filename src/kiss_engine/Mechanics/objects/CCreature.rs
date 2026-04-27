//! CCreature — creature models (CHeadless, etc.) with idle animation.
//!
//! In the original KISS Psycho Circus engine, creatures are AI-driven enemies.
//! This module places them as models in the world and plays their idle
//! animation by swapping pre-baked, uniformly-spaced keyframe meshes in the
//! draw group.  Frames are generated at load time with bone-interpolated
//! sampling (slerp/lerp) so playback is smooth with no runtime cost.

use crate::types::DrawGroup;

#[derive(Debug, Clone)]
pub struct CreatureObject {
    pub position: [f32; 3],
    pub draw_group: usize,
    pub type_name: String,
    /// first_index into the global index buffer for each animation frame
    pub frame_first_indices: Vec<u32>,
    /// index_count (same for all frames)
    pub index_count: u32,
    /// Total number of uniformly-spaced frames
    pub num_frames: usize,
    /// Total animation duration in seconds
    pub anim_duration: f32,
    /// Current animation time (loops)
    pub anim_time: f32,
    /// Current displayed frame index
    pub current_frame: usize,
}

pub fn parse(pos: [f32; 3], dg: usize, type_name: &str) -> CreatureObject {
    CreatureObject {
        position: pos,
        draw_group: dg,
        type_name: type_name.to_string(),
        frame_first_indices: Vec::new(),
        index_count: 0,
        num_frames: 0,
        anim_duration: 0.0,
        anim_time: 0.0,
        current_frame: 0,
    }
}

/// Set up animation data from pre-computed frame info.
pub fn set_animation(
    creature: &mut CreatureObject,
    frame_first_indices: Vec<u32>,
    index_count: u32,
    _keyframe_times_ms: &[u32],
    duration_ms: u32,
) {
    creature.num_frames = frame_first_indices.len();
    creature.frame_first_indices = frame_first_indices;
    creature.index_count = index_count;
    creature.anim_duration = duration_ms as f32 / 1000.0;
}

/// Advance the idle animation and update the draw group to show the current frame.
pub fn update(creature: &mut CreatureObject, dt: f32, draw_groups: &mut Vec<DrawGroup>) {
    if creature.num_frames <= 1 || creature.anim_duration <= 0.0 {
        return;
    }

    creature.anim_time += dt;
    // Loop the animation
    while creature.anim_time >= creature.anim_duration {
        creature.anim_time -= creature.anim_duration;
    }

    // Uniform spacing: pick frame directly from normalised time
    let frame_idx = ((creature.anim_time / creature.anim_duration) * creature.num_frames as f32)
        .floor() as usize
        % creature.num_frames;

    if frame_idx != creature.current_frame {
        creature.current_frame = frame_idx;
        if let Some(dg) = draw_groups.get_mut(creature.draw_group) {
            if frame_idx < creature.frame_first_indices.len() {
                dg.first_index = creature.frame_first_indices[frame_idx];
                dg.index_count = creature.index_count;
            }
        }
    }
}
