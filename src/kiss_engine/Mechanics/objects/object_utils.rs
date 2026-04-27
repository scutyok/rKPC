//******************************************************************/
// 
// Shared utility functions for all game-object modules.
//
// Provides DAT property readers, draw-group helpers, and math utilities that
// every per-object file can import without circular dependencies.
//
//******************************************************************/

use cgmath::Matrix4;

use crate::dat::{PropertyValue, WorldObject};
use crate::types::DrawGroup;

//******************************************************************/
//
// DAT property readers
//
//******************************************************************/

pub fn prop_float(obj: Option<&WorldObject>, name: &str, default: f32) -> f32 {
    obj.and_then(|o| o.get_property(name))
        .and_then(|v| if let PropertyValue::Float(f) = v { Some(*f) } else { None })
        .unwrap_or(default)
}

pub fn prop_bool(obj: Option<&WorldObject>, name: &str, default: bool) -> bool {
    obj.and_then(|o| o.get_property(name))
        .and_then(|v| if let PropertyValue::Bool(b) = v { Some(*b != 0) } else { None })
        .unwrap_or(default)
}

pub fn prop_string(obj: Option<&WorldObject>, name: &str) -> String {
    obj.and_then(|o| o.get_property(name))
        .and_then(|v| if let PropertyValue::String(s) = v { Some(s.clone()) } else { None })
        .unwrap_or_default()
}

//******************************************************************/
//
// Read a Vector3 / Color property and return it as `[x, y, z]`, or `None`.
//
//******************************************************************/

/*pub fn prop_vector(obj: Option<&WorldObject>, name: &str) -> Option<[f32; 3]> {
    obj.and_then(|o| o.get_property(name))
        .and_then(|v| match v {
            PropertyValue::Vector(c) | PropertyValue::Color(c) => Some([c.x, c.y, c.z]),
            _ => None,
        })
}*/

//******************************************************************/
//
// Draw-group helpers
//
//******************************************************************/

/// Set a draw group's `index_count` to 0, hiding it from the renderer.
pub fn hide_draw_group(draw_groups: &mut Vec<DrawGroup>, idx: usize) {
    if let Some(dg) = draw_groups.get_mut(idx) {
        dg.index_count = 0;
    }
}

/// Set (or clear) the per-object model-matrix override on a draw group.
pub fn set_draw_group_matrix(
    draw_groups: &mut Vec<DrawGroup>,
    idx: usize,
    mat: Option<[[f32; 4]; 4]>,
) {
    if let Some(dg) = draw_groups.get_mut(idx) {
        dg.model_matrix = mat;
    }
}

//******************************************************************/
//
// Math helpers
//
//******************************************************************/

/// Euclidean distance between two 3-D points.
#[inline]
pub fn dist3(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

// Convert a `cgmath::Matrix4<f32>` to a raw `[[f32; 4]; 4]` push-constant array.
#[inline]
pub fn matrix4_to_array(m: Matrix4<f32>) -> [[f32; 4]; 4] {
    [
        [m.x.x, m.x.y, m.x.z, m.x.w],
        [m.y.x, m.y.y, m.y.z, m.y.w],
        [m.z.x, m.z.y, m.z.z, m.z.w],
        [m.w.x, m.w.y, m.w.z, m.w.w],
    ]
}

// Linearly interpolate `remaining / total`, clamped to [0, 1].
#[inline]
pub fn time_to_fraction(remaining: f32, total: f32) -> f32 {
    if total <= 0.0 {
        0.0
    } else {
        (remaining / total).clamp(0.0, 1.0)
    }
}
