use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::fs;
use std::path::Path;

pub fn read_json_input(raw: &str) -> Result<Value> {
    let trimmed = raw.trim();
    if trimmed.starts_with('@') {
        let path = trimmed.trim_start_matches('@');
        return read_json_file(Path::new(path));
    }

    if Path::new(trimmed).exists() {
        return read_json_file(Path::new(trimmed));
    }

    serde_json::from_str(trimmed).context("invalid JSON input")
}

fn read_json_file(path: &Path) -> Result<Value> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("read json file {}", path.display()))?;
    let value: Value = serde_json::from_str(&contents)
        .map_err(|err| anyhow!("invalid JSON in {}: {err}", path.display()))?;
    Ok(value)
}
