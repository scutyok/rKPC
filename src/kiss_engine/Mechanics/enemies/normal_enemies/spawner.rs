#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Spawner {
    pub hp: Option<u32>,
    pub damage: Option<u32>,
    pub element: Option<&'static str>,
    pub score: Option<u32>,
    pub source: Option<&'static str>,
}

impl Spawner {
    pub fn new() -> Self {
        Self {
            hp: None,
            damage: None,
            element: Some("Nightmare"),
            score: Some(0),
        }
    }

    pub fn name(&self) -> &'static str { "Spawner" }
}
