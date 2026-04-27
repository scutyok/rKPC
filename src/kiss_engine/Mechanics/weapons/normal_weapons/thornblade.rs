#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Thornblade {
    pub power: Option<u32>,
    pub element: Option<&'static str>,
    pub description: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl Thornblade {
    pub fn new() -> Self {
        Self {
            power: None,
            element: Some("Water"),
            description: Some("Melee weapon; two swings per attack"),
        }
    }

    pub fn name(&self) -> &'static str { "Thornblade" }
}
