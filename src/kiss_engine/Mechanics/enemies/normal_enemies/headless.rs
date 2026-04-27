#![allow(dead_code)]

use cgmath::{vec3, Matrix4};
use crate::object_utils::matrix4_to_array;

/// Events emitted by the Headless AI for the game to act upon.
#[derive(Debug, Clone)]
pub enum HeadlessEvent {
    /// Ask the game to execute a named SCR trigger (resolved by GameObjectManager).
    ExecuteScript(String),
    /// Deal damage to the player (amount in game units).
    DealDamage(f32),
    /// The headless moved; provide the new world-space translation matrix.
    Moved([[f32; 4]; 4]),
    /// Animation state changed (Idle / Charging / Death) — useful to pick an animation clip.
    AnimationChange(HeadlessAnim),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadlessAnim {
    Idle,
    Charge,
    Death,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiState {
    Idle,
    Charging,
    Dead,
}

#[derive(Debug, Clone)]
pub struct Headless {
    /// World position (Vulkan/renderer coordinates) of the entity.
    pub position: [f32; 3],
    /// Draw group index that contains this creature's base geometry (optional).
    pub draw_group: Option<usize>,
    /// Bounding box dims (X, Y, Z) in renderer units.
    pub bounding_box: [f32; 3],
    /// Optional SCR trigger names to run at state transitions.
    pub charge_start_script: Option<String>,
    pub charge_end_script: Option<String>,
    pub death_script: Option<String>,
    /// Current health.
    pub health: i32,
    /// Current AI state.
    pub ai_state: AiState,
    /// Current animation selection (synthesised from AI state).
    pub current_anim: HeadlessAnim,
    /// Optional creature animation object (wraps precomputed keyframe indices).
    pub creature_anim: Option<crate::CCreature::CreatureObject>,
    /// Charge movement speed (units / second).
    pub charge_speed: f32,
    /// How far the headless can detect the player.
    pub detect_range: f32,
    /// Distance at which the headless damages the player when charging.
    pub attack_range: f32,
}

impl Headless {
    /// Create a new headless at `pos` with optional draw group index.
    pub fn new(pos: [f32; 3], draw_group: Option<usize>) -> Self {
        Self {
            position: pos,
            draw_group,
            bounding_box: [20.0, 29.5, 20.0],
            charge_start_script: None,
            charge_end_script: None,
            death_script: None,
            health: 50,
            ai_state: AiState::Idle,
            current_anim: HeadlessAnim::Idle,
            creature_anim: None,
            charge_speed: 15.0,
            detect_range: 12.0,
            attack_range: 5.0,
        }
    }

    /// Helper to register (set) script names referenced by the original
    /// C implementation (charge start/end and death).
    pub fn set_scripts<S: Into<String>>(&mut self, start: Option<S>, end: Option<S>, death: Option<S>) {
        self.charge_start_script = start.map(|s| s.into());
        self.charge_end_script = end.map(|s| s.into());
        self.death_script = death.map(|s| s.into());
    }

    /// Attach a `CCreature::CreatureObject` to provide idle keyframe animation.
    pub fn set_creature(&mut self, creature: crate::CCreature::CreatureObject) {
        self.creature_anim = Some(creature);
    }

    /// Advance creature idle animation (delegates to `CCreature::update`).
    pub fn update_animation(&mut self, dt: f32, draw_groups: &mut Vec<crate::types::DrawGroup>) {
        if let Some(cre) = &mut self.creature_anim {
            crate::CCreature::update(cre, dt, draw_groups);
        }
    }

    /// Apply damage to the headless. Returns true when it dies this call.
    pub fn apply_damage(&mut self, amount: f32) -> bool {
        self.health -= amount as i32;
        if self.health <= 0 && self.ai_state != AiState::Dead {
            self.ai_state = AiState::Dead;
            self.current_anim = HeadlessAnim::Death;
            return true;
        }
        false
    }

    /// Build a model matrix for the current translation so render code can
    /// apply it to the draw group's `model_matrix` push-constant.
    pub fn model_matrix(&self) -> [[f32; 4]; 4] {
        matrix4_to_array(Matrix4::from_translation(vec3(self.position[0], self.position[1], self.position[2])))
    }

    /// Basic line-of-sight / detection test. Currently only distance-based.
    pub fn can_see_player(&self, player_pos: [f32; 3]) -> bool {
        let dx = player_pos[0] - self.position[0];
        let dy = player_pos[1] - self.position[1];
        let dz = player_pos[2] - self.position[2];
        let dist = (dx*dx + dy*dy + dz*dz).sqrt();
        dist <= self.detect_range
    }

    /// Advance the AI by `dt` seconds with the given player position.
    /// Returns a list of events the caller should process (script triggers,
    /// damage to player, and model-matrix updates for rendering).
    pub fn update(&mut self, dt: f32, player_pos: [f32; 3]) -> Vec<HeadlessEvent> {
        let mut out = Vec::new();

        // If dead, run death script once and then do nothing.
        if let AiState::Dead = self.ai_state {
            if let Some(script) = &self.death_script {
                out.push(HeadlessEvent::ExecuteScript(script.clone()));
            }
            // Ensure we only run death script once by clearing it.
            self.death_script = None;
            return out;
        }

        match self.ai_state {
            AiState::Idle => {
                /*if self.can_see_player(player_pos) {
                    self.ai_state = AiState::Charging;
                    self.current_anim = HeadlessAnim::Charge;
                    if let Some(scr) = &self.charge_start_script {
                        out.push(HeadlessEvent::ExecuteScript(scr.clone()));
                    }
                    out.push(HeadlessEvent::AnimationChange(self.current_anim));
                }*/
            }
            AiState::Charging => {
                // Move toward player on XY plane (preserve Z axis)
                let dir_x = player_pos[0] - self.position[0];
                let dir_y = player_pos[1] - self.position[1];
                let dir_z = player_pos[2] - self.position[2];
                let horiz_len = (dir_x*dir_x + dir_y*dir_y).sqrt();
                if horiz_len > 1e-6 {
                    let nx = dir_x / horiz_len;
                    let ny = dir_y / horiz_len;
                    let step = self.charge_speed * dt;
                    self.position[0] += nx * step;
                    self.position[1] += ny * step;
                }
                // Push moved matrix for renderer to apply
                out.push(HeadlessEvent::Moved(self.model_matrix()));

                // Check for attack range (use full 3D distance)
                let dist = (dir_x*dir_x + dir_y*dir_y + dir_z*dir_z).sqrt();
                if dist <= self.attack_range {
                    // Deal damage to the player
                    out.push(HeadlessEvent::DealDamage(10.0));
                    self.ai_state = AiState::Idle;
                    self.current_anim = HeadlessAnim::Idle;
                    if let Some(scr) = &self.charge_end_script {
                        out.push(HeadlessEvent::ExecuteScript(scr.clone()));
                    }
                    out.push(HeadlessEvent::AnimationChange(self.current_anim));
                }
            }
            AiState::Dead => {}
        }

        out
    }

    /// Convenience: set detection/attack ranges and charge speed (tuning)
    pub fn tune(&mut self, detect: f32, attack: f32, speed: f32) {
        self.detect_range = detect;
        self.attack_range = attack;
        self.charge_speed = speed;
    }
}

