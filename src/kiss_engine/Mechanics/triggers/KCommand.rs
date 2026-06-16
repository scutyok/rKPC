#[derive(Debug, Clone, PartialEq)]
pub enum TriggerCommand {
    Trigger,    // "TRIGGER" — fire the target
    Lock,       // "LOCK"
    Unlock,     // "UNLOCK"
    Remove,     // "REMOVE" — delete the target entity
    Reset,      // "RESET"
}

impl TriggerCommand {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "TRIGGER" => Some(Self::Trigger),
            "LOCK"    => Some(Self::Lock),
            "UNLOCK"  => Some(Self::Unlock),
            "REMOVE"  => Some(Self::Remove),
            "RESET"   => Some(Self::Reset),
            _         => None,
        }
    }
}