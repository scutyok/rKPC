//******************************************************************/
//
// CWater / CWaterVolume — axis-aligned water volume.
//
// While the player is inside:
//  * Gravity is reduced to 25 % of normal.
//  * Movement speed is halved.
//  * Jump acts as a swim-up impulse.
//
// No mesh / draw group — purely a physics zone.
// Volume half-extents are read from the `Dims` DAT property.
//
//******************************************************************/

use crate::dat::{PropertyValue, WorldObject};

#[derive(Debug, Clone)]
pub struct WaterObject {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl WaterObject {
    pub fn contains(&self, p: [f32; 3]) -> bool {
        p[0] >= self.min[0]
            && p[0] <= self.max[0]
            && p[1] >= self.min[1]
            && p[1] <= self.max[1]
            && p[2] >= self.min[2]
            && p[2] <= self.max[2]
    }
}

//******************************************************************/
//
// DAT construction
//
//******************************************************************/

pub fn parse(pos: [f32; 3], props: Option<&WorldObject>, scale: f32) -> WaterObject {
    let ext = props
        .and_then(|o| o.get_property("Dims"))
        .and_then(|v| {
            if let PropertyValue::Vector(vec) = v {
                // Lithtech (x,y,z) → Vulkan (x,z,y)
                Some([vec.x * scale * 0.5, vec.z * scale * 0.5, vec.y * scale * 0.5])
            } else {
                None
            }
        })
        .unwrap_or([1.0, 1.0, 1.0]);
    WaterObject {
        min: [pos[0] - ext[0], pos[1] - ext[1], pos[2] - ext[2]],
        max: [pos[0] + ext[0], pos[1] + ext[1], pos[2] + ext[2]],
    }
}
