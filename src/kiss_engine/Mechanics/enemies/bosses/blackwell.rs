#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Blackwell {
    pub hp: Option<u32>,
    pub damage: Option<u32>,
    pub element: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl Blackwell {
    pub fn new() -> Self {
        Self {
            hp: None,
            damage: None,
            element: Some("Fire"),
        }
    }

    pub fn name(&self) -> &'static str { "Blackwell" }
    pub fn is_boss(&self) -> bool { true }
}
