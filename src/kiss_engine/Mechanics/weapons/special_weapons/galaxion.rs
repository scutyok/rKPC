#![allow(dead_code)]

#[derive(Debug, Clone)]
pub struct Galaxion {
    pub power: Option<u32>,
    pub element: Option<&'static str>,
    pub description: Option<&'static str>,
    pub source: Option<&'static str>,
}

impl Galaxion {
    pub fn new() -> Self {
        Self {
            power: Some(80),
            element: Some("Air"),
            description: Some("Creates a portal/black hole on solid surfaces; best vs some bosses"),
        }
    }

    pub fn name(&self) -> &'static str { "Galaxion" }
}
