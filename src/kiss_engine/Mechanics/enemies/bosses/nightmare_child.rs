#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct NightmareChild {
    pub hp: Option<u32>,
    pub damage: Option<u32>,
    pub element: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl NightmareChild {
    pub fn new() -> Self {
        Self {
            hp: None,
            damage: None,
            element: None,
        }
    }

    pub fn name(&self) -> &'static str { "Nightmare Child" }
    pub fn is_boss(&self) -> bool { true }
}
