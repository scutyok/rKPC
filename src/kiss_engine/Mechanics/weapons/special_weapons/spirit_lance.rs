#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct SpiritLance {
    pub power: Option<u32>,
    pub element: Option<&'static str>,
    pub description: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl SpiritLance {
    pub fn new() -> Self {
        Self {
            power: Some(50),
            element: Some("Earth"),
            description: Some("Accurate piercing hitscan weapon"),
        }
    }

    pub fn name(&self) -> &'static str { "Spirit Lance" }
}
