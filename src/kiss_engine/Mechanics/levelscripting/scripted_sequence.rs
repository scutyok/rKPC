//******************************************************************/
//
// SCR script parser and runner for KISS Psycho Circus scripted sequences.
//
// SCR files contain timed commands like:
//   2.8 trigger_door door01 open
//   3.2 trigger_generic Doorless01
//
// This module parses the commands we can act on and runs them at the right
// time offsets when the sequence is triggered.
//
//******************************************************************/


use cgmath::{Matrix4, vec3};

use crate::object_utils::matrix4_to_array;
use crate::types::DrawGroup;

//******************************************************************/
//
// Script commands
//
//******************************************************************/


#[derive(Debug, Clone)]
pub enum ScriptCommand {
    /// Open a BSP door sub-model by name.
    TriggerDoorOpen { door_name: String },
}

#[derive(Debug, Clone)]
pub struct TimedCommand {
    pub time: f32,
    pub command: ScriptCommand,
}

//******************************************************************/
//
// SCR parser
//
//******************************************************************/

// Parse an SCR script file into a list of timed commands.
// Only commands we can act on are extracted; the rest are ignored.
pub fn parse_scr(contents: &str) -> Vec<TimedCommand> {
    let mut commands = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let time: f32 = match parts[0].parse() {
            Ok(t) => t,
            Err(_) => continue,
        };
        match parts[1] {
            "trigger_door" if parts.len() >= 4 && parts[3] == "open" => {
                commands.push(TimedCommand {
                    time,
                    command: ScriptCommand::TriggerDoorOpen {
                        door_name: parts[2].to_lowercase(),
                    },
                });
            }
            _ => {} // Ignore commands we don't handle yet
        }
    }
    commands.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    commands
}

//******************************************************************/
//
// BSP Door
//
//******************************************************************/


/// Slide speed in Vulkan units per second.
const BSP_DOOR_SPEED: f32 = 3.0;

#[derive(Debug, Clone, PartialEq)]
pub enum BspDoorState {
    Closed,
    Opening,
    Open,
}

//******************************************************************/
//
// A BSP sub-model door that slides upward when opened.
//
//******************************************************************/

#[derive(Debug, Clone)]
pub struct BspDoor {
    /// Lowercase name matching the BSP world model name (e.g. "door01").
    pub name: String,
    /// Draw group indices for this BSP sub-model.
    pub draw_groups: Vec<usize>,
    /// Slide distance in Vulkan units (computed from mesh height).
    pub slide_distance: f32,
    /// Current slide offset [0..slide_distance].
    pub slide_offset: f32,
    pub state: BspDoorState,
    /// Range of collision vertex indices [start..end) that belong to this door.
    pub collision_vertex_range: Option<(usize, usize)>,
    /// Original (closed) positions of the collision vertices, so we can re-derive the slid positions.
    pub collision_base_positions: Vec<cgmath::Vector3<f32>>,
}

impl BspDoor {
    pub fn open(&mut self) {
        if self.state == BspDoorState::Closed {
            self.state = BspDoorState::Opening;
        }
    }

    pub fn update(&mut self, dt: f32, draw_groups: &mut Vec<DrawGroup>) {
        if self.state == BspDoorState::Opening {
            self.slide_offset += dt * BSP_DOOR_SPEED;
            if self.slide_offset >= self.slide_distance {
                self.slide_offset = self.slide_distance;
                self.state = BspDoorState::Open;
            }
            let m = matrix4_to_array(
                Matrix4::from_translation(vec3(0.0, 0.0, self.slide_offset)),
            );
            for &dg in &self.draw_groups {
                if dg < draw_groups.len() {
                    draw_groups[dg].model_matrix = Some(m);
                }
            }
        }
    }

    /// Update the collision mesh positions to follow the door's slide offset.
    pub fn update_collision(&self, collision_positions: &mut [cgmath::Vector3<f32>]) {
        if let Some((start, end)) = self.collision_vertex_range {
            if self.slide_offset > 0.0 && end <= collision_positions.len() {
                for (i, base_pos) in self.collision_base_positions.iter().enumerate() {
                    collision_positions[start + i] = cgmath::Vector3::new(
                        base_pos.x,
                        base_pos.y,
                        base_pos.z + self.slide_offset,
                    );
                }
            }
        }
    }
}

//******************************************************************/
//
// Script Runner
//
//******************************************************************/


#[derive(Debug, Clone)]
pub struct ScriptRunner {
    pub commands: Vec<TimedCommand>,
    pub elapsed: f32,
    pub next_index: usize,
    pub running: bool,
}

impl ScriptRunner {
    pub fn new(commands: Vec<TimedCommand>) -> Self {
        Self {
            commands,
            elapsed: 0.0,
            next_index: 0,
            running: false,
        }
    }

    pub fn start(&mut self) {
        if !self.running {
            self.running = true;
            self.elapsed = 0.0;
            self.next_index = 0;
        }
    }

    /// Advance the script clock and return any commands whose time has been reached.
    pub fn update(&mut self, dt: f32) -> Vec<ScriptCommand> {
        if !self.running {
            return Vec::new();
        }
        self.elapsed += dt;
        let mut fired = Vec::new();
        while self.next_index < self.commands.len()
            && self.commands[self.next_index].time <= self.elapsed
        {
            fired.push(self.commands[self.next_index].command.clone());
            self.next_index += 1;
        }
        if self.next_index >= self.commands.len() {
            self.running = false;
        }
        fired
    }
}
