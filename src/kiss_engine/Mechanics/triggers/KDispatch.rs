pub fn dispatch_message(target_name: &str, message: &str, world: &mut WorldContext) {
    let cmd = match TriggerCommand::parse(message) {
        Some(c) => c,
        None    => return,
    };

    let targets: Vec<EntityId> = world.registry.find(target_name).to_vec();

    for entity_id in targets {
        match cmd {
            TriggerCommand::Remove  => world.remove_entity(entity_id),
            TriggerCommand::Trigger => world.fire_trigger(entity_id),
            TriggerCommand::Lock    => world.send_command(entity_id, TriggerCommand::Lock),
            TriggerCommand::Unlock  => world.send_command(entity_id, TriggerCommand::Unlock),
            TriggerCommand::Reset   => world.send_command(entity_id, TriggerCommand::Reset),
        }
    }
}