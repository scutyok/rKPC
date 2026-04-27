#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Grinder {
    pub hp: Option<u32>,
    pub damage: Option<u32>,
    pub element: Option<&'static str>,
    pub score: Option<u32>,
    pub source: Option<&'static str>,
}

impl Grinder {
    pub fn new() -> Self {
        Self {
            hp: None,
            damage: None,
            element: Some("Fire"),
            score: Some(2000),
        }
    }

    pub fn name(&self) -> &'static str { "Grinder" }
}
