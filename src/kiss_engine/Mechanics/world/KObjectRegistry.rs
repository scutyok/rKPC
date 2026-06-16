use std::collections::HashMap;

/// Name → list of entity IDs. Replaces g_pServerDE->FindNamedObjects().
pub struct ObjectRegistry {
    by_name: HashMap<String, Vec<EntityId>>,
}

impl ObjectRegistry {
    pub fn find(&self, name: &str) -> &[EntityId] {
        self.by_name
            .get(&name.to_ascii_uppercase())
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn register(&mut self, name: &str, id: EntityId) {
        self.by_name
            .entry(name.to_ascii_uppercase())
            .or_default()
            .push(id);
    }

    pub fn remove(&mut self, id: EntityId) {
        for ids in self.by_name.values_mut() {
            ids.retain(|&e| e != id);
        }
    }
}