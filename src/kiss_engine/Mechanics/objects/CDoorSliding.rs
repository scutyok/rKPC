//! CDoorSliding — proximity-triggered or switch-triggered sliding door.
//!
//! State machine:
//!   Closed → Opening (slide at DOOR_SLIDE_SPEED Vulkan units/s)
//!          → Open   (holds for 3 s when auto_close is set)
//!          → Closing (reverse slide)
//!          → Closed
//!
//! DAT properties read:
//!   `SlideDir`       (Vector3) — direction to slide in Lithtech coords.
//!   `SlideDistance`  (Float)   — total travel distance (Lithtech units × scale).
//!   `TriggerRadius`  (Float)   — auto-open radius (Lithtech units × scale).
//!   `TriggerTarget`  (String)  — switch name that opens this door.
//!   `AutoClose`      (Bool)    — whether door closes again after 3 s.

use cgmath::{Matrix4, vec3};

use crate::dat::{PropertyValue, WorldObject};
use crate::object_utils::{dist3, matrix4_to_array, prop_bool, prop_float, prop_string,
    set_draw_group_matrix};
use crate::types::DrawGroup;

const DOOR_SLIDE_SPEED: f32 = 3.0; // Vulkan units/s

// ─── State ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DoorState {
    Closed,
    Opening { progress: f32 },
    Open,
    Closing { progress: f32 },
}

// ─── DoorObject ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DoorObject {
    pub position: [f32; 3],
    pub state: DoorState,
    /// Direction the door slides (Vulkan coords, normalised).
    pub slide_dir: [f32; 3],
    pub slide_distance: f32,
    pub trigger_radius: f32,
    pub trigger_name: String,
    pub draw_group: usize,
    pub auto_close: bool,
    pub open_hold_timer: f32,
    /// Collision AABB half-extents (from mesh bounds).
    pub half_extents: [f32; 3],
    /// Center offset of the AABB from position.
    pub aabb_center: [f32; 3],
}

impl DoorObject {
    pub fn open(&mut self) {
        if self.state == DoorState::Closed {
            self.state = DoorState::Opening { progress: 0.0 };
        }
    }

    pub fn close(&mut self) {
        if self.state == DoorState::Open {
            self.state = DoorState::Closing { progress: 1.0 };
        }
    }

    /// Returns the current AABB min/max accounting for door slide.
    /// Returns None if the door is fully open.
    pub fn current_aabb(&self) -> Option<([f32; 3], [f32; 3])> {
        if self.state == DoorState::Open {
            return None;
        }
        let t = self.slide_fraction() * self.slide_distance;
        let d = self.slide_dir;
        let cx = self.aabb_center[0] + d[0] * t;
        let cy = self.aabb_center[1] + d[1] * t;
        let cz = self.aabb_center[2] + d[2] * t;
        Some((
            [cx - self.half_extents[0], cy - self.half_extents[1], cz - self.half_extents[2]],
            [cx + self.half_extents[0], cy + self.half_extents[1], cz + self.half_extents[2]],
        ))
    }

    fn slide_fraction(&self) -> f32 {
        match self.state {
            DoorState::Closed => 0.0,
            DoorState::Open => 1.0,
            DoorState::Opening { progress } | DoorState::Closing { progress } => progress,
        }
    }

    fn model_matrix(&self) -> [[f32; 4]; 4] {
        let t = self.slide_fraction() * self.slide_distance;
        let d = self.slide_dir;
        let m: Matrix4<f32> = Matrix4::from_translation(vec3(d[0] * t, d[1] * t, d[2] * t));
        matrix4_to_array(m)
    }
}

// ─── DAT construction ────────────────────────────────────────────────────────

pub fn parse(
    pos: [f32; 3],
    props: Option<&WorldObject>,
    draw_group: usize,
    scale: f32,
    mesh_bounds: Option<([f32; 3], [f32; 3])>,
) -> DoorObject {
    let raw_dir = props
        .and_then(|o| o.get_property("SlideDir"))
        .and_then(|v| {
            if let PropertyValue::Vector(vec) = v {
                Some([vec.x, vec.z, vec.y]) // Lithtech → Vulkan coord swizzle
            } else {
                None
            }
        })
        .unwrap_or([1.0, 0.0, 0.0]);

    let len = (raw_dir[0] * raw_dir[0] + raw_dir[1] * raw_dir[1] + raw_dir[2] * raw_dir[2]).sqrt();
    let slide_dir = if len > 1e-6 {
        [raw_dir[0] / len, raw_dir[1] / len, raw_dir[2] / len]
    } else {
        [1.0, 0.0, 0.0]
    };

    let (half_extents, aabb_center) = if let Some((bmin, bmax)) = mesh_bounds {
        (
            [(bmax[0] - bmin[0]) * 0.5, (bmax[1] - bmin[1]) * 0.5, (bmax[2] - bmin[2]) * 0.5],
            [(bmin[0] + bmax[0]) * 0.5, (bmin[1] + bmax[1]) * 0.5, (bmin[2] + bmax[2]) * 0.5],
        )
    } else {
        ([0.5, 0.5, 1.0], pos)
    };

    DoorObject {
        position: pos,
        state: DoorState::Closed,
        slide_dir,
        slide_distance: prop_float(props, "SlideDistance", 200.0) * scale,
        trigger_radius: prop_float(props, "TriggerRadius", 300.0) * scale,
        trigger_name: prop_string(props, "TriggerTarget"),
        draw_group,
        auto_close: prop_bool(props, "AutoClose", false),
        open_hold_timer: 0.0,
        half_extents,
        aabb_center,
    }
}

// ─── Per-frame update ────────────────────────────────────────────────────────

pub fn update(
    door: &mut DoorObject,
    dt: f32,
    player_pos: [f32; 3],
    draw_groups: &mut Vec<DrawGroup>,
) {
    match door.state {
        DoorState::Opening { ref mut progress } => {
            *progress += dt * DOOR_SLIDE_SPEED / door.slide_distance;
            if *progress >= 1.0 {
                *progress = 1.0;
                door.state = DoorState::Open;
                door.open_hold_timer = 3.0;
            }
        }
        DoorState::Closing { ref mut progress } => {
            *progress -= dt * DOOR_SLIDE_SPEED / door.slide_distance;
            if *progress <= 0.0 {
                *progress = 0.0;
                door.state = DoorState::Closed;
            }
        }
        DoorState::Open if door.auto_close => {
            door.open_hold_timer -= dt;
            if door.open_hold_timer <= 0.0 {
                door.close();
            }
        }
        _ => {}
    }

    // Proximity auto-open when no explicit switch link.
    if door.trigger_name.is_empty()
        && door.state == DoorState::Closed
        && dist3(player_pos, door.position) < door.trigger_radius
    {
        door.open();
    }

    set_draw_group_matrix(draw_groups, door.draw_group, Some(door.model_matrix()));
}
