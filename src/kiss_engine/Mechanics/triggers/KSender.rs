

/// Replaces IsKindOf(hObjClass, hPlayerObj) etc.
#[derive(Debug, Clone, PartialEq)]
pub enum SenderKind {
    Player,
    Ai,
    Trigger,   // relay
    Object,    // generic world object
}

pub struct Sender {
    pub entity_id: EntityId,
    pub kind: SenderKind,
    pub name: String,           // for NamedObjectActivate check
    pub inventory_keys: Vec<String>, // for key-lock check
}