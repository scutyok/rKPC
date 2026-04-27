#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Stargrave {
    pub hp: Option<u32>,
    pub damage: Option<u32>,
    pub element: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl Stargrave {
    pub fn new() -> Self {
        Self {
            hp: None,
            damage: None,
            element: Some("Air"),
        }
    }

    pub fn name(&self) -> &'static str { "Stargrave" }
    pub fn is_boss(&self) -> bool { true }
}
