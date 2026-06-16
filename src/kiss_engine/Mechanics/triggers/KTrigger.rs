use super::*;

impl Trigger {
    // ----------------------------------------------------------------
    // Replaces ValidateSender
    // ----------------------------------------------------------------
    pub fn validate_sender(&self, sender: &Sender) -> bool {
        // Relay triggers always pass first
        if self.trigger_relay_activate && sender.kind == SenderKind::Trigger {
            return self.check_named_object(sender);
        }

        let allowed = match sender.kind {
            SenderKind::Player  => self.player_activate && self.check_lock(sender),
            SenderKind::Ai      => self.ai_activate,
            SenderKind::Object  => self.object_activate,
            SenderKind::Trigger => false, // already handled above
        };

        if allowed { self.check_named_object(sender) } else { false }
    }

    fn check_named_object(&self, sender: &Sender) -> bool {
        if self.named_object_activate {
            if let Some(ref required) = self.activation_object_name {
                return sender.name.eq_ignore_ascii_case(required);
            }
        }
        true
    }

    fn check_lock(&self, sender: &Sender) -> bool {
        if !self.locked { return true; }
        // Check inventory for required key
        if let Some(ref key) = self.key_name {
            sender.inventory_keys.iter().any(|k| k.eq_ignore_ascii_case(key))
        } else {
            false
        }
    }

    // ----------------------------------------------------------------
    // Replaces ObjectTouch / HandleTriggerMsg("TRIGGER")
    // ----------------------------------------------------------------
    pub fn try_activate(
        &mut self,
        sender: &Sender,
        now: f32,
        world: &mut WorldContext,
    ) {
        if !self.active || !self.validate_sender(sender) { return; }

        self.current_activation += 1;
        if self.current_activation < self.activation_count {
            return; // counter trigger, not enough hits yet
        }
        self.current_activation = 0;

        if let Some(ref sound) = self.activation_sound.clone() {
            world.audio.play_at(sound, world.positions[&sender.entity_id], self.sound_radius);
        }

        let max_delay = self.messages.iter().map(|m| m.delay).fold(0.0_f32, f32::max);

        if max_delay > 0.0 {
            // Schedule per-slot delayed sends
            for (i, msg) in self.messages.iter().enumerate() {
                if msg.delay > 0.0 {
                    self.pending_delays[i] = Some(now); // start time
                } else {
                    dispatch_message(&msg.target_name, &msg.message, world);
                }
            }
        } else {
            self.send_all(world);
        }

        self.active = false;
        self.last_fire_time = now;
    }

    // ----------------------------------------------------------------
    // Replaces Update (polls pending delays each frame)
    // ----------------------------------------------------------------
    pub fn update(&mut self, now: f32, world: &mut WorldContext) {
        let any_pending = self.pending_delays.iter().any(|d| d.is_some());

        if !any_pending {
            // All done — reset after reset_time
            if !self.active && (now - self.last_fire_time) >= self.reset_time {
                self.active = true;
            }
            return;
        }

        let mut all_sent = true;
        for i in 0..MAX_TRIGGER_MESSAGES {
            if let Some(start) = self.pending_delays[i] {
                let delay = self.messages.get(i).map(|m| m.delay).unwrap_or(0.0);
                if now >= start + delay {
                    if let Some(msg) = self.messages.get(i) {
                        dispatch_message(&msg.target_name.clone(), &msg.message.clone(), world);
                    }
                    self.pending_delays[i] = None;
                } else {
                    all_sent = false;
                }
            }
        }

        if all_sent {
            self.last_fire_time = now;
        }
    }

    // ----------------------------------------------------------------
    // Replaces SendMessages (all slots, immediate)
    // ----------------------------------------------------------------
    fn send_all(&mut self, world: &mut WorldContext) {
        if self.sending { return; }
        self.sending = true;
        for msg in self.messages.clone() {
            dispatch_message(&msg.target_name, &msg.message, world);
        }
        self.sending = false;
    }

    // ----------------------------------------------------------------
    // Replaces HandleTriggerMsg for LOCK / UNLOCK / RESET
    // ----------------------------------------------------------------
    pub fn handle_command(&mut self, cmd: TriggerCommand, world: &mut WorldContext) {
        match cmd {
            TriggerCommand::Lock   => self.locked = true,
            TriggerCommand::Unlock => {
                if self.locked {
                    if let Some(ref snd) = self.unlocked_sound.clone() {
                        world.audio.play_local(snd);
                    }
                }
                self.locked = false;
            },
            TriggerCommand::Reset  => {
                self.active = true;
                self.pending_delays = [None; MAX_TRIGGER_MESSAGES];
            },
            _ => {}
        }
    }
}