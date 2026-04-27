#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Draco {
    pub power: Option<u32>,
    pub element: Option<&'static str>,
    pub description: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl Draco {
    pub fn new() -> Self {
        Self {
            power: None,
            element: Some("Fire"),
            description: Some("Flamethrower-style super weapon; continuous damage while firing"),
        }
    }

    pub fn name(&self) -> &'static str { "Draco" }
}
