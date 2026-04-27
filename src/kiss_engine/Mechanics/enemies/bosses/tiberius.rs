#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Tiberius {
    pub hp: Option<u32>,
    pub damage: Option<u32>,
    pub element: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl Tiberius {
    pub fn new() -> Self {
        Self {
            hp: None,
            damage: None,
            element: Some("Earth"),
        }
    }

    pub fn name(&self) -> &'static str { "Tiberius" }
    pub fn is_boss(&self) -> bool { true }
}
