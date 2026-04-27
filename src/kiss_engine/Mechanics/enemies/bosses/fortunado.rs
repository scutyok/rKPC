#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Fortunado {
    pub hp: Option<u32>,
    pub damage: Option<u32>,
    pub element: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl Fortunado {
    pub fn new() -> Self {
        Self {
            hp: None,
            damage: None,
            element: Some("Water"),
        }
    }

    pub fn name(&self) -> &'static str { "Fortunado" }
    pub fn is_boss(&self) -> bool { true }
}
