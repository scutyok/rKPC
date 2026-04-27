#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct FatLady {
    pub hp: Option<u32>,
    pub damage: Option<u32>,
    pub element: Option<&'static str>,
    pub score: Option<u32>,
    pub source: Option<&'static str>,
}

impl FatLady {
    pub fn new() -> Self {
        Self {
            hp: None,
            damage: None,
            element: Some("Water"),
            score: Some(400),
        }
    }

    pub fn name(&self) -> &'static str { "Fat Lady" }
}
