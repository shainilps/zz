use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Registry {
    pub tags: Vec<String>,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub tags: Vec<String>,
}

fn registry_path() -> Result<PathBuf, String> {
    let mut dir = dirs::config_dir().ok_or("could not determine config directory")?;
    dir.push("zz");
    dir.push("registry.toml");
    Ok(dir)
}

pub fn load() -> Result<Registry, String> {
    let path = registry_path()?;

    if !path.exists() {
        return Ok(Registry::default());
    }

    let text =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    toml::from_str(&text).map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

pub fn save(registry: &Registry) -> Result<(), String> {
    let path = registry_path()?;

    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    }

    let text = toml::to_string_pretty(registry)
        .map_err(|e| format!("failed to serialize registry: {e}"))?;

    let tmp_path = path.with_extension("toml.tmp");

    fs::write(&tmp_path, text)
        .map_err(|e| format!("failed to write {}: {e}", tmp_path.display()))?;

    fs::rename(&tmp_path, &path).map_err(|e| {
        format!(
            "failed to rename {} to {}: {e}",
            tmp_path.display(),
            path.display()
        )
    })?;

    Ok(())
}
