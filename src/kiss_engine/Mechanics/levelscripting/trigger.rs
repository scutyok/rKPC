use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::dat::PropertyValue;
use crate::scripted_sequence::{parse_dat_command, ScriptCommand};
use crate::util::geometry::AABB;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerActivation {
    Touch,
    Use,
    Script,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerSource {
    BspVolume,
    BspSwitch,
    DatObject,
}

// TriggerAabb moved to `crate::util::math::TriggerAabb`

#[derive(Debug, Clone)]
pub struct TriggerDef {
    pub name: String,
    pub source: TriggerSource,
    pub activation: TriggerActivation,
    pub once: bool,
    pub enabled: bool,
    pub activated: bool,
    pub volume: Option<AABB>,
    pub center: Option<[f32; 3]>,
    pub use_radius: f32,
    pub actions: Vec<ScriptCommand>,
}

impl TriggerDef {
    pub fn can_fire(&self) -> bool {
        self.enabled && (!self.once || !self.activated)
    }

    pub fn contains(&self, pos: [f32; 3]) -> bool {
        self.volume.map(|v: AABB| v.contains(pos.into())).unwrap_or(false)
    }

    pub fn can_use_from(&self, pos: [f32; 3]) -> bool {
        if let Some(volume) = self.volume {
            return volume.contains(pos.into());
        }
        if let Some(center) = self.center {
            let dx = pos[0] - center[0];
            let dy = pos[1] - center[1];
            let dz = pos[2] - center[2];
            return (dx * dx + dy * dy + dz * dz).sqrt() <= self.use_radius;
        }
        false
    }
}

pub struct TriggerFactory<'a> {
    file_stem: &'a str,
    dat: &'a crate::dat::DatFile,
    trigger_volumes: &'a [(String, [f32; 3], [f32; 3])],
    bsp_submodels: &'a [(String, [f32; 3], Vec<usize>, f32)],
    script_object_targets: HashMap<String, Vec<String>>,
}

impl<'a> TriggerFactory<'a> {
    pub fn new(
        file_stem: &'a str,
        dat: &'a crate::dat::DatFile,
        trigger_volumes: &'a [(String, [f32; 3], [f32; 3])],
        bsp_submodels: &'a [(String, [f32; 3], Vec<usize>, f32)],
    ) -> Self {
        let mut factory = Self {
            file_stem,
            dat,
            trigger_volumes,
            bsp_submodels,
            script_object_targets: HashMap::new(),
        };
        factory.index_script_objects();
        factory
    }

    pub fn build(self) -> Vec<TriggerDef> {
        let mut out = Vec::new();
        self.build_volume_triggers(&mut out);
        self.build_bsp_switch_triggers(&mut out);
        out
    }

    fn index_script_objects(&mut self) {
        let empty_targets: HashMap<String, Vec<String>> = HashMap::new();
        for obj in &self.dat.objects {
            if obj.type_name != "CScriptObject" {
                continue;
            }
            let obj_name = match obj.get_property("Name") {
                Some(PropertyValue::String(s)) => s.to_lowercase(),
                _ => continue,
            };

            let mut scripts = Vec::new();
            for cmd in command_properties(obj, 8) {
                for action in parse_dat_command(cmd, self.file_stem, &empty_targets) {
                    if let ScriptCommand::StartScript { script_name } = action {
                        scripts.push(script_name);
                    }
                }
            }
            if !scripts.is_empty() {
                self.script_object_targets.insert(obj_name, scripts);
            }
        }
    }

    fn build_volume_triggers(&self, out: &mut Vec<TriggerDef>) {
        for (name, min, max) in self.trigger_volumes {
            let name_lc = name.to_lowercase();
            let dat_obj = self.find_dat_object(&name_lc);
            let mut actions = dat_obj
                .map(|obj| self.actions_from_object_commands(obj, 4))
                .unwrap_or_default();
            let has_dat_actions = !actions.is_empty();

            if name_lc == "cslime2" {
                actions = vec![ScriptCommand::StartScript {
                    script_name: "microphone".to_string(),
                }];
            }

            if actions.is_empty() {
                actions.push(ScriptCommand::StartScript {
                    script_name: name_lc.clone(),
                });
            }

            out.push(TriggerDef {
                name: name_lc,
                source: TriggerSource::BspVolume,
                activation: if has_dat_actions {
                    TriggerActivation::Touch
                } else {
                    TriggerActivation::Use
                },
                once: true,
                enabled: true,
                activated: false,
                volume: Some(AABB { min: (*min).into(), max: (*max).into() }),
                center: None,
                use_radius: 0.0,
                actions,
            });
        }
    }

    fn build_bsp_switch_triggers(&self, out: &mut Vec<TriggerDef>) {
        for (name, center, _draw_groups, _z_height) in self.bsp_submodels {
            let name_lc = name.to_lowercase();
            if !name_lc.starts_with("cswitchslide") && !name_lc.starts_with("cswitchrotating") {
                continue;
            }

            let dat_obj = self.dat.objects.iter().find(|o| {
                (o.type_name == "CSwitchSlide" || o.type_name == "CSwitchRotating")
                    && matches!(o.get_property("Name"), Some(PropertyValue::String(n)) if n.to_lowercase() == name_lc)
            });
            let actions = dat_obj
                .map(|obj| self.actions_from_object_commands(obj, 4))
                .unwrap_or_default();

            if actions.is_empty() {
                continue;
            }

            out.push(TriggerDef {
                name: name_lc,
                source: TriggerSource::BspSwitch,
                activation: TriggerActivation::Use,
                once: true,
                enabled: true,
                activated: false,
                volume: None,
                center: Some(*center),
                use_radius: 2.0,
                actions,
            });
        }
    }

    fn actions_from_object_commands(
        &self,
        obj: &crate::dat::WorldObject,
        max_commands: usize,
    ) -> Vec<ScriptCommand> {
        let mut actions = Vec::new();
        for cmd in command_properties(obj, max_commands) {
            actions.extend(parse_dat_command(
                cmd,
                self.file_stem,
                &self.script_object_targets,
            ));
        }
        actions
    }

    fn find_dat_object(&self, name: &str) -> Option<&crate::dat::WorldObject> {
        self.dat.objects.iter().find(|o| {
            matches!(o.get_property("Name"), Some(PropertyValue::String(n)) if n.to_lowercase() == name)
        })
    }
}

fn command_properties(obj: &crate::dat::WorldObject, max_commands: usize) -> Vec<&str> {
    let mut out = Vec::new();
    for i in 1..=max_commands {
        let cmd_key = format!("command{}", i);
        if let Some(PropertyValue::String(cmd)) = obj.get_property(&cmd_key) {
            if !cmd.trim().is_empty() {
                out.push(cmd.as_str());
            }
        }
    }
    out
}

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
