#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Stump {
    pub hp: Option<u32>,
    pub damage: Option<u32>,
    pub element: Option<&'static str>,
    pub score: Option<u32>,
    pub source: Option<&'static str>,
}

impl Stump {
    pub fn new() -> Self {
        Self {
            hp: None,
            damage: None,
            element: Some("Fire"),
            score: Some(45),
        }
    }

    pub fn name(&self) -> &'static str { "Stump" }
}
