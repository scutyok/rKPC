#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Scourge {
    pub power: Option<u32>,
    pub element: Option<&'static str>,
    pub description: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl Scourge {
    pub fn new() -> Self {
        Self {
            power: Some(3),
            element: Some("Earth"),
            description: Some("Accurate hitscan-like weapon; can pull player to glowing items"),
        }
    }

    pub fn name(&self) -> &'static str { "Scourge" }
}
