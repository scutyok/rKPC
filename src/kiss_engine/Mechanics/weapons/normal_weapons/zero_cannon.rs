#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct ZeroCannon {
    pub power: Option<u32>,
    pub element: Option<&'static str>,
    pub description: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl ZeroCannon {
    pub fn new() -> Self {
        Self {
            power: Some(1),
            element: Some("Water"),
            description: Some("Chaingun-inspired water cannon; ice shards per shot"),
        }
    }

    pub fn name(&self) -> &'static str { "Zero Cannon" }
}
