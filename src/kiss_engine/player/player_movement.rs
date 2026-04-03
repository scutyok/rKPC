//! Quake / Blood 2-style acceleration-based player movement.
//!
//! Ported from Lithtech Jupiter engine (`CMoveMgr::UpdateNormalMotion`) with
//! Quake-style air-strafe physics.  Key features:
//!
//! * Acceleration + friction model (not direct-velocity)
//! * Air control with reduced acceleration (strafe-jumping works)
//! * Smooth deceleration via ground friction
//! * Proper jump with gravity integration



/// Movement tuning constants — derived from Blood 2 / Quake defaults, scaled
/// to the 0.01 Lithtech→Vulkan coordinate system used by this renderer.
///
/// Blood 2 originals (in Lithtech units, ~1 unit = 1cm):
///   RunVel 420, JumpVel 550, Gravity -1300, Friction 5, BaseMoveAccel ~3000
///
/// Our world is 100× smaller (scale 0.01), velocities are 0.01× those values.
pub mod consts {
    /// Maximum ground speed the player can accelerate to (units/s).
    pub const MAX_SPEED: f32 = 4.0;
    /// Ground acceleration (units/s²).
    pub const GROUND_ACCEL: f32 = 50.0;
    /// Air acceleration (units/s²) — low value gives Quake-style air-strafe.
    pub const AIR_ACCEL: f32 = 8.0;
    /// Ground friction coefficient — higher = less slippery.
    pub const FRICTION: f32 = 14.0;
    /// Gravity acceleration (units/s²).
    pub const GRAVITY: f32 = 22.0;
    /// Initial upward velocity when jumping (units/s).
    pub const JUMP_VELOCITY: f32 = 6.5;
    /// Terminal falling speed (units/s).  Prevents tunnelling.
    pub const TERMINAL_VELOCITY: f32 = 20.0;
    /// Fly-mode speed multiplier for direct velocity (units/s).
    pub const FLY_SPEED: f32 = 10.0;
    /// Minimum speed threshold below which velocity is zeroed (prevents drift).
    pub const STOP_SPEED: f32 = 0.5;
}

use consts::*;

/// XY velocity vector carried across frames.
pub type Vec2 = [f32; 2];

/// Persistent movement state that lives alongside the camera.
#[derive(Clone, Debug)]
pub struct MovementState {
    /// Horizontal velocity (X, Y) in world units/s.
    pub velocity: Vec2,
    /// Vertical velocity (Z) in world units/s.
    pub z_vel: f32,
    /// True when the player is on a walkable surface.
    pub on_ground: bool,
    /// True the frame the player initiated a jump (prevents re-triggering).
    pub jumped: bool,
}

impl Default for MovementState {
    fn default() -> Self {
        Self {
            velocity: [0.0, 0.0],
            z_vel: 0.0,
            on_ground: false,
            jumped: false,
        }
    }
}

/// Input wish-directions distilled from keyboard state, ready for the physics tick.
pub struct MoveInput {
    /// Unit-length wish direction on the XY plane, or zero if no keys pressed.
    pub wish_dir: [f32; 2],
    /// Whether the player is pressing *any* horizontal movement key.
    pub wish_move: bool,
    /// Jump requested this frame.
    pub jump: bool,
}

/// Build a `MoveInput` from the current keyboard flags and camera orientation.
///
/// `front_flat` and `right` should be unit-length XY vectors derived from the camera yaw.
pub fn build_move_input(
    forward: bool,
    backward: bool,
    left: bool,
    right_key: bool,
    jump: bool,
    front_flat: [f32; 2],
    right: [f32; 2],
) -> MoveInput {
    let mut dx = 0.0_f32;
    let mut dy = 0.0_f32;
    if forward {
        dx += front_flat[0];
        dy += front_flat[1];
    }
    if backward {
        dx -= front_flat[0];
        dy -= front_flat[1];
    }
    if left {
        dx -= right[0];
        dy -= right[1];
    }
    if right_key {
        dx += right[0];
        dy += right[1];
    }
    let len = (dx * dx + dy * dy).sqrt();
    if len > 1e-6 {
        MoveInput {
            wish_dir: [dx / len, dy / len],
            wish_move: true,
            jump,
        }
    } else {
        MoveInput {
            wish_dir: [0.0, 0.0],
            wish_move: false,
            jump,
        }
    }
}

// ─── Physics tick ────────────────────────────────────────────────────────────

/// Run one physics tick.  Modifies `state` in place and returns the XY
/// position delta to apply (Z delta is applied via `state.z_vel`).
///
/// This function does NOT touch the camera position; the caller applies the
/// returned delta and then runs collision resolution.
pub fn tick(state: &mut MovementState, input: &MoveInput, dt: f32) -> [f32; 3] {
    if dt <= 0.0 {
        return [0.0, 0.0, 0.0];
    }

    // ── Horizontal movement ──────────────────────────────────────────────
    if state.on_ground {
        // Apply friction first (Quake order: friction → accelerate)
        apply_friction(state, dt);
        // Then accelerate along wish direction
        if input.wish_move {
            accelerate(state, input.wish_dir, MAX_SPEED, GROUND_ACCEL, dt);
        }
    } else {
        // Air movement: reduced acceleration gives strafe-jump feel
        if input.wish_move {
            air_accelerate(state, input.wish_dir, MAX_SPEED, AIR_ACCEL, dt);
        }
    }

    // ── Jump ─────────────────────────────────────────────────────────────
    if input.jump && state.on_ground && !state.jumped {
        state.z_vel = JUMP_VELOCITY;
        state.on_ground = false;
        state.jumped = true;
    }
    if !input.jump {
        state.jumped = false;
    }

    // ── Gravity ──────────────────────────────────────────────────────────
    if !state.on_ground {
        state.z_vel -= GRAVITY * dt;
        if state.z_vel < -TERMINAL_VELOCITY {
            state.z_vel = -TERMINAL_VELOCITY;
        }
    }

    // ── Compute position delta ───────────────────────────────────────────
    let dz = state.z_vel * dt;
    [state.velocity[0] * dt, state.velocity[1] * dt, dz]
}

/// Quake-style ground friction: reduces speed proportionally, with a minimum
/// deceleration floor (`STOP_SPEED`) so the player actually comes to a halt.
fn apply_friction(state: &mut MovementState, dt: f32) {
    let speed = (state.velocity[0] * state.velocity[0]
        + state.velocity[1] * state.velocity[1])
        .sqrt();
    if speed < 1e-6 {
        state.velocity = [0.0, 0.0];
        return;
    }

    // Quake formula: drop = max(speed, STOP_SPEED) * FRICTION * dt
    let control = if speed < STOP_SPEED { STOP_SPEED } else { speed };
    let drop = control * FRICTION * dt;
    let new_speed = (speed - drop).max(0.0);
    let scale = new_speed / speed;
    state.velocity[0] *= scale;
    state.velocity[1] *= scale;
}

/// Quake-style ground acceleration: accelerates along `wish_dir` up to
/// `max_speed`, clamped so you can't exceed it.
fn accelerate(state: &mut MovementState, wish_dir: [f32; 2], max_speed: f32, accel: f32, dt: f32) {
    // Current speed projected onto wish direction
    let current_speed = state.velocity[0] * wish_dir[0] + state.velocity[1] * wish_dir[1];
    let add_speed = max_speed - current_speed;
    if add_speed <= 0.0 {
        return; // already at or above max speed in this direction
    }
    let accel_speed = (accel * dt * max_speed).min(add_speed);
    state.velocity[0] += accel_speed * wish_dir[0];
    state.velocity[1] += accel_speed * wish_dir[1];
}

/// Quake-style air acceleration: same as `accelerate` but typically called
/// with a much lower `accel` value, enabling strafe-jumping.
fn air_accelerate(state: &mut MovementState, wish_dir: [f32; 2], max_speed: f32, accel: f32, dt: f32) {
    // Cap wish-speed to 1.0 u/s in air (Quake's `sv_maxairspeed 30` equivalent)
    // This is the key to strafe-jumping: low air wish-speed allows velocity
    // to grow when strafing perpendicular to current velocity.
    let wish_speed = max_speed.min(2.0);
    let current_speed = state.velocity[0] * wish_dir[0] + state.velocity[1] * wish_dir[1];
    let add_speed = wish_speed - current_speed;
    if add_speed <= 0.0 {
        return;
    }
    let accel_speed = (accel * dt * wish_speed).min(add_speed);
    state.velocity[0] += accel_speed * wish_dir[0];
    state.velocity[1] += accel_speed * wish_dir[1];
}

// ─── Fly mode ────────────────────────────────────────────────────────────────

/// Fly-mode position delta (noclip): direct velocity, no physics.
pub fn fly_tick(
    forward: bool,
    backward: bool,
    left: bool,
    right_key: bool,
    up: bool,
    down: bool,
    front: [f32; 3],
    right_vec: [f32; 3],
    dt: f32,
) -> [f32; 3] {
    let s = FLY_SPEED * dt;
    let mut dx = 0.0_f32;
    let mut dy = 0.0_f32;
    let mut dz = 0.0_f32;
    if forward  { dx += front[0] * s; dy += front[1] * s; dz += front[2] * s; }
    if backward { dx -= front[0] * s; dy -= front[1] * s; dz -= front[2] * s; }
    if left     { dx -= right_vec[0] * s; dy -= right_vec[1] * s; }
    if right_key{ dx += right_vec[0] * s; dy += right_vec[1] * s; }
    if up       { dz += s; }
    if down     { dz -= s; }
    [dx, dy, dz]
}
