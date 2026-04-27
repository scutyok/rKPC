#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Strongman {
    pub hp: Option<u32>,
    pub damage: Option<u32>,
    pub element: Option<&'static str>,
    pub score: Option<u32>,
    pub source: Option<&'static str>,
}

impl Strongman {
    pub fn new() -> Self {
        Self {
            hp: None,
            damage: None,
            element: Some("Earth"),
            score: Some(2000),
        }
    }

    pub fn name(&self) -> &'static str { "Strongman" }
}
