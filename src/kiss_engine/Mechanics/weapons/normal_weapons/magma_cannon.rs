#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct MagmaCannon {
    pub power: Option<u32>,
    pub element: Option<&'static str>,
    pub description: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl MagmaCannon {
    pub fn new() -> Self {
        Self {
            power: Some(4),
            element: Some("Fire"),
            description: Some("Shotgun-style; multiple pellets per shot"),
        }
    }

    pub fn name(&self) -> &'static str { "Magma Cannon" }
}
