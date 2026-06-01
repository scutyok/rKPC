use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::dat::PropertyValue;

#[derive(Serialize, Debug, Clone)]
pub struct TriggerInfo {
    pub source: String,
    pub name: String,
    pub position: Option<[f32; 3]>,
    pub rotation: Option<[f32; 4]>,
    pub aabb_min: Option<[f32; 3]>,
    pub aabb_max: Option<[f32; 3]>,
    pub properties: HashMap<String, JsonValue>,
}

fn prop_to_json(p: &PropertyValue) -> JsonValue {
    match p {
        PropertyValue::String(s) => json!(s),
        PropertyValue::Vector(v) => json!([v.x, v.y, v.z]),
        PropertyValue::Color(c) => json!([c.x, c.y, c.z]),
        PropertyValue::Float(f) => json!(f),
        PropertyValue::Bool(b) => json!((*b) != 0),
        PropertyValue::Flags(f) => json!(f),
        PropertyValue::LongInt(l) => json!(l),
        PropertyValue::Rotation(q) => json!([q.w, q.x, q.y, q.z]),
        PropertyValue::UnknownInt(u) => json!(u),
    }
}

/// Collect triggers from DAT `objects` and BSP `trigger_volumes`.
pub fn collect_triggers(dat: &crate::dat::DatFile, trigger_volumes: &[(String, [f32; 3], [f32; 3])], scale: f32) -> Vec<TriggerInfo> {
    let mut out = Vec::new();

    let keywords = [
        "trigger",
        "volume",
        "script",
        "teleport",
        "portal",
        "zone",
        "pickup",
        "death",
        "damage",
        "kill",
        "lava",
        "slime",
    ];

    for obj in &dat.objects {
        let tn_lc = obj.type_name.to_lowercase();
        let mut is_trigger = false;
        for kw in &keywords {
            if tn_lc.contains(kw) {
                is_trigger = true;
                break;
            }
        }

        if !is_trigger {
            if obj.get_property("Script").is_some()
                || obj.get_property("TargetName").is_some()
                || obj.get_property("Name").is_some()
            {
                is_trigger = true;
            }
        }

        if !is_trigger {
            continue;
        }

        let position = obj.get_position().map(|p| [p.x * scale, p.z * scale, p.y * scale]);
        let rotation = obj.get_rotation().map(|q| [q.w, q.x, q.y, q.z]);

        let mut props = HashMap::new();
        for prop in &obj.properties {
            props.insert(prop.name.clone(), prop_to_json(&prop.value));
        }

        let name = obj
            .get_property("Name")
            .and_then(|pv| match pv {
                PropertyValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| obj.type_name.clone());

        out.push(TriggerInfo {
            source: "object".to_string(),
            name,
            position,
            rotation,
            aabb_min: None,
            aabb_max: None,
            properties: props,
        });
    }

    for (name, vmin, vmax) in trigger_volumes.iter() {
        out.push(TriggerInfo {
            source: "world_model".to_string(),
            name: name.clone(),
            position: None,
            rotation: None,
            aabb_min: Some(*vmin),
            aabb_max: Some(*vmax),
            properties: HashMap::new(),
        });
    }

    out
}

/// Export triggers as a pretty JSON file next to the DAT file.
pub fn export_triggers_json<P: AsRef<Path>>(triggers: &[TriggerInfo], dat_path: P) -> Result<()> {
    let dat_path = dat_path.as_ref();
    let stem = dat_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("level");
    let out_name = format!("{}.triggers.json", stem);
    let out_path = dat_path.with_file_name(out_name);
    let s = serde_json::to_string_pretty(triggers).context("serializing triggers to json")?;
    fs::write(&out_path, s).with_context(|| format!("writing triggers to {:?}", out_path))?;
    Ok(())
}
