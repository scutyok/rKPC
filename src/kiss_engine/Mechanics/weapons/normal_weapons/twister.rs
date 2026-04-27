#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Twister {
    pub power: Option<u32>,
    pub element: Option<&'static str>,
    pub description: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl Twister {
    pub fn new() -> Self {
        Self {
            power: None,
            element: Some("Air"),
            description: Some("Fast melee weapon; damages multiple targets in front"),
        }
    }

    pub fn name(&self) -> &'static str { "Twister" }
}
