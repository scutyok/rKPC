use crate::dat::{WorldObject, PropertyValue};


//******************************************************************/
//
// A point light extracted from the DAT file, in Vulkan world coordinates (scaled).
//
//******************************************************************/

#[derive(Debug, Clone, Copy)]
pub struct Light {
    /// World-space position (Vulkan coords: X=LithtechX, Y=LithtechZ, Z=LithtechY, all * scale)
    pub position: [f32; 3],
    /// Light radius of influence (scaled)
    pub radius: f32,
    /// RGB color (normalized 0..1)
    pub color: [f32; 3],
    /// Brightness multiplier
    pub intensity: f32,
}

//******************************************************************/
//
// Extract all Light objects from DAT world objects, converting to Vulkan coordinates.
//
// The coordinate swap matches dat_mesh.rs: Lithtech (X,Y,Z) → Vulkan (X, Z, Y).
// Positions and radii are multiplied by `scale` (typically 0.01).
//
//******************************************************************/

pub fn extract_lights_from_objects(objects: &[WorldObject], scale: f32) -> Vec<Light> {
    let mut lights = Vec::new();

    for obj in objects {
        // Lithtech engines use various light type names
        let tn = obj.type_name.to_ascii_lowercase();
        let is_light = tn == "light"
            || tn == "dirlight"
            || tn == "baselight"
            || tn == "pointlight"
            || tn == "dynamiclight"
            || tn == "staticlight"
            || tn == "objectlight"
            || tn.contains("light");
        if !is_light {
            continue;
        }

        // Position (required)
        let pos = match obj.get_position() {
            Some(p) => p,
            None => continue,
        };

        // LightRadius (defaults to 500 Lithtech units if missing)
        let radius = match obj.get_property("LightRadius") {
            Some(PropertyValue::Float(r)) => *r,
            _ => 500.0,
        };

        // Color: Lithtech stores as 0..255 Vector3 or Color
        let (r, g, b) = match obj.get_property("Color") {
            Some(PropertyValue::Color(c)) | Some(PropertyValue::Vector(c)) => {
                (c.x / 255.0, c.y / 255.0, c.z / 255.0)
            }
            _ => match obj.get_property("LightColor") {
                Some(PropertyValue::Color(c)) | Some(PropertyValue::Vector(c)) => {
                    (c.x / 255.0, c.y / 255.0, c.z / 255.0)
                }
                _ => (1.0, 1.0, 1.0),
            },
        };

        // BrightScale (optional brightness multiplier)
        let bright_scale = match obj.get_property("BrightScale") {
            Some(PropertyValue::Float(s)) => *s,
            _ => 1.0,
        };

        // Coordinate conversion: Lithtech → Vulkan (same swap as dat_mesh.rs)
        lights.push(Light {
            position: [
                pos.x * scale,
                pos.z * scale, // Lithtech Z → Vulkan Y
                pos.y * scale, // Lithtech Y → Vulkan Z
            ],
            radius: radius * scale,
            color: [r, g, b],
            intensity: bright_scale * 0.5,
        });
    }

    if !lights.is_empty() {
        println!("Extracted {} lights from DAT objects", lights.len());
    } else {
        println!("WARNING: No light objects found in DAT file");
    }

    lights
}
