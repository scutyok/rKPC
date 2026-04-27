#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct MeanieBeanie {
    pub hp: Option<u32>,
    pub damage: Option<u32>,
    pub element: Option<&'static str>,
    pub score: Option<u32>,
    pub source: Option<&'static str>,
}

impl MeanieBeanie {
    pub fn new() -> Self {
        Self {
            hp: None,
            damage: None,
            element: Some("Air"),
            score: Some(750),
        }
    }

    pub fn name(&self) -> &'static str { "Meanie Beanie" }
}
