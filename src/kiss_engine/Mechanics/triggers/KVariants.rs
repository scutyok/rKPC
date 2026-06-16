

// ---- ExitTrigger ----
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ExitType {
    EndOfWorld,
    EndOfEpisode,
    EndOfSubWorld,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExitTrigger {
    pub base: Trigger,
    pub next_world: Option<String>,
    pub start_point: String,
    pub exit_type: ExitType,
}

impl ExitTrigger {
    /// Replaces SendMessages — writes directly to GameState, no network
    pub fn fire(&self, state: &mut GameState) {
        state.pending_level_transition = Some(LevelTransition {
            next_world:  self.next_world.clone(),
            start_point: self.start_point.clone(),
            exit_type:   self.exit_type.clone(),
        });
    }
}

// ---- ObjectivesTrigger ---- (from previous session, kept here for completeness)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ObjectivesTrigger {
    pub base: Trigger,
    pub title: String,
    pub text: String,           // '|' = line break
    pub sound: Option<String>,
    pub resource_id: u32,
}

impl ObjectivesTrigger {
    pub fn fire(&self, state: &mut GameState, audio: &mut AudioSystem) {
        state.current_objective = Some(ObjectivesState {
            resource_id: self.resource_id,
            title:       self.title.clone(),
            text:        self.text.clone(),
            is_active:   true,
        });
        if let Some(ref snd) = self.sound {
            audio.play_local(snd);
        }
    }
}

// ---- ToggleTrigger ----
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToggleTrigger {
    pub base: Trigger,
    pub on: bool,
    /// When on==true, use off_messages instead of base.messages
    pub off_messages: Vec<TriggerMessage>,
}

impl ToggleTrigger {
    pub fn fire(&mut self, world: &mut WorldContext) {
        let msgs = if self.on {
            self.off_messages.clone()
        } else {
            self.base.messages.clone()
        };
        for msg in msgs {
            dispatch_message(&msg.target_name, &msg.message, world);
        }
        self.on = !self.on;
    }
}