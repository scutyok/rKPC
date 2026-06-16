

pub const MAX_TRIGGER_MESSAGES: usize = 10;

/// Replaces all HSTRING fields. Owned Strings, zero manual free.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TriggerMessage {
    pub target_name: String,    // TargetName1..10
    pub message: String,        // MessageName1..10 — "TRIGGER", "LOCK", "REMOVE", etc.
    pub delay: f32,             // MessageDelay1..10
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Trigger {
    // Identity
    pub name: String,

    // Bounding box for touch detection
    pub dims: glam::Vec3,

    // Activation rules
    pub active: bool,
    pub locked: bool,
    pub touch_activate: bool,
    pub player_activate: bool,
    pub ai_activate: bool,
    pub object_activate: bool,
    pub trigger_relay_activate: bool,
    pub named_object_activate: bool,
    pub activation_object_name: Option<String>,

    // Counter: fire only after N touches
    pub activation_count: u32,
    pub current_activation: u32,

    // Timing
    pub reset_time: f32,
    pub last_fire_time: f32,

    // Messages to dispatch on fire (up to 10 slots)
    pub messages: Vec<TriggerMessage>,

    // Per-slot delay state
    pub pending_delays: [Option<f32>; MAX_TRIGGER_MESSAGES], // Some(start_time) if pending

    // Sounds
    pub activation_sound: Option<String>,
    pub sound_radius: f32,
    pub locked_sound: Option<String>,
    pub unlocked_sound: Option<String>,
    pub key_name: Option<String>,

    // Reentrancy guard (replaces m_bSending)
    pub sending: bool,
}

impl Default for Trigger {
    fn default() -> Self {
        Self {
            name: String::new(),
            dims: glam::Vec3::ZERO,
            active: true,
            locked: false,
            touch_activate: true,
            player_activate: true,
            ai_activate: true,
            object_activate: false,
            trigger_relay_activate: true,
            named_object_activate: false,
            activation_object_name: None,
            activation_count: 1,
            current_activation: 0,
            reset_time: 0.0,
            last_fire_time: 0.0,
            messages: Vec::new(),
            pending_delays: [None; MAX_TRIGGER_MESSAGES],
            activation_sound: None,
            sound_radius: 200.0,
            locked_sound: None,
            unlocked_sound: None,
            key_name: None,
            sending: false,
        }
    }
}