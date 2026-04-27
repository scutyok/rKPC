#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Punisher {
    pub power: Option<u32>,
    pub element: Option<&'static str>,
    pub description: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl Punisher {
    pub fn new() -> Self {
        Self {
            power: None,
            element: Some("Fire"),
            description: Some("Slowest melee weapon but highest single-swing damage"),
        }
    }

    pub fn name(&self) -> &'static str { "Punisher" }
}
