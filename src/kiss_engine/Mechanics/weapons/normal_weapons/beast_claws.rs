#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct BeastClaws {
    pub power: Option<u32>,
    pub element: Option<&'static str>,
    pub description: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl BeastClaws {
    pub fn new() -> Self {
        Self {
            power: None,
            element: Some("Earth"),
            description: Some("Fast melee weapon with short reach"),
        }
    }

    pub fn name(&self) -> &'static str { "Beast Claws" }
}
