//! CCreature — statically placed creature models (CHeadless, etc.).
//!
//! In the original KISS Psycho Circus engine, creatures are AI-driven enemies.
//! For now, this module just places them as static models in the world so they
//! are visible. No AI or animation is implemented.

#[derive(Debug, Clone)]
pub struct CreatureObject {
    pub position: [f32; 3],
    pub draw_group: usize,
    pub type_name: String,
}

pub fn parse(pos: [f32; 3], dg: usize, type_name: &str) -> CreatureObject {
    CreatureObject {
        position: pos,
        draw_group: dg,
        type_name: type_name.to_string(),
    }
}
