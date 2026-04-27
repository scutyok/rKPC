#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Ballbuster {
    pub hp: Option<u32>,
    pub damage: Option<u32>,
    pub element: Option<&'static str>,
    pub score: Option<u32>,
    pub source: Option<&'static str>,
}

impl Ballbuster {
    pub fn new() -> Self {
        Self {
            hp: None,
            damage: None,
            element: Some("Earth"),
            score: Some(700),
        }
    }

    pub fn name(&self) -> &'static str { "Ballbuster" }
}
