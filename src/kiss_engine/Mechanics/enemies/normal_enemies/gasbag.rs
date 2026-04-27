#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Gasbag {
    pub hp: Option<u32>,
    pub damage: Option<u32>,
    pub element: Option<&'static str>,
    pub score: Option<u32>,
    pub source: Option<&'static str>,
}

impl Gasbag {
    pub fn new() -> Self {
        Self {
            hp: None,
            damage: None,
            element: Some("Air"),
            score: Some(125),
        }
    }

    pub fn name(&self) -> &'static str { "Gasbag" }
}
