use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug)]
pub struct Settings {
    pub dynamic_lighting: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dynamic_lighting: true,
        }
    }
}

impl Settings {
    pub fn load() -> Result<Self> {
        let path = Self::path();
        if path.exists() {
            let s = fs::read_to_string(&path).with_context(|| format!("reading settings from {:?}", path))?;
            let settings: Settings = serde_json::from_str(&s).context("parsing settings json")?;
            Ok(settings)
        } else {
            Ok(Settings::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("creating settings directory {:?}", dir))?;
        }
        let s = serde_json::to_string_pretty(self).context("serializing settings")?;
        fs::write(&path, s).with_context(|| format!("writing settings to {:?}", path))?;
        Ok(())
    }

    fn path() -> PathBuf {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                return dir.join("settings.json");
            }
        }
        PathBuf::from("settings.json")
    }
}
