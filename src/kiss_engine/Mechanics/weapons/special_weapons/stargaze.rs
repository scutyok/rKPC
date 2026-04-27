#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Stargaze {
    pub power: Option<u32>,
    pub element: Option<&'static str>,
    pub description: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl Stargaze {
    pub fn new() -> Self {
        Self {
            power: Some(80),
            element: Some("Water"),
            description: Some("Damages multiple enemies in front; high power per shot"),
        }
    }

    pub fn name(&self) -> &'static str { "Stargaze" }
}
