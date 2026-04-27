#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Windblade {
    pub power: Option<u32>,
    pub element: Option<&'static str>,
    pub description: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl Windblade {
    pub fn new() -> Self {
        Self {
            power: Some(5),
            element: Some("Air"),
            description: Some("Rocket-launcher style; slow projectile that explodes on impact"),
        }
    }

    pub fn name(&self) -> &'static str { "Windblade" }
}
