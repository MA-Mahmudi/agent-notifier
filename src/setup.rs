use crate::config;
use anyhow::Context;
use chrono::Utc;
use serde_json::{json, Value};
use std::path::Path;

const MARKER: &str = "agent-notifier@mmmohebi.github.io";
const EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PermissionRequest",
    "Notification",
    "Stop",
    "StopFailure",
    "Interrupt",
    "SessionEnd",
];

pub fn setup(apply: bool, binary: &Path) -> anyhow::Result<()> {
    let home = dirs::home_dir().context("home directory unavailable")?;
    let targets = [
        ("codex", home.join(".codex/hooks.json")),
        ("claude", home.join(".claude/settings.json")),
    ];
    for (source, path) in targets {
        println!(
            "{} {} hooks in {}",
            if apply { "Installing" } else { "Would install" },
            source,
            path.display()
        );
        if apply {
            edit(&path, source, binary, false)?
        }
    }
    if !apply {
        println!("Preview only. Run `agent-notifier setup --apply` to install.")
    }
    Ok(())
}
pub fn uninstall(binary: &Path) -> anyhow::Result<()> {
    let home = dirs::home_dir().context("home directory unavailable")?;
    for (source, path) in [
        ("codex", home.join(".codex/hooks.json")),
        ("claude", home.join(".claude/settings.json")),
    ] {
        if path.exists() {
            edit(&path, source, binary, true)?
        }
    }
    Ok(())
}
fn command(source: &str, binary: &Path) -> String {
    format!("{} hook {} # {}", shell_quote(binary), source, MARKER)
}
fn shell_quote(p: &Path) -> String {
    format!("'{}'", p.display().to_string().replace('\'', "'\\''"))
}
fn edit(path: &Path, source: &str, binary: &Path, remove: bool) -> anyhow::Result<()> {
    let original = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        "{}".into()
    };
    let mut root: Value = serde_json::from_str(&original)
        .with_context(|| format!("refusing to modify malformed JSON: {}", path.display()))?;
    let obj = root
        .as_object_mut()
        .context("settings root must be an object")?;
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("hooks must be an object")?;
    let cmd = command(source, binary);
    for event in EVENTS {
        if source == "codex" && matches!(*event, "Notification" | "StopFailure") {
            continue;
        }
        if source == "claude" && *event == "Interrupt" {
            continue;
        }
        let groups = hooks
            .entry(event.to_string())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .context("hook event must be an array")?;
        for group in groups.iter_mut() {
            if let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                handlers.retain(|h| {
                    h.get("command")
                        .and_then(Value::as_str)
                        .map(|x| !x.contains(MARKER))
                        .unwrap_or(true)
                });
            }
        }
        groups.retain(|g| {
            g.get("hooks")
                .and_then(Value::as_array)
                .map(|a| !a.is_empty())
                .unwrap_or(true)
        });
        if !remove {
            groups.push(json!({"hooks":[{"type":"command","command":cmd,"timeout":3}]}));
        }
    }
    let rendered = serde_json::to_string_pretty(&root)? + "\n";
    if rendered == original {
        return Ok(());
    }
    if path.exists() {
        let backup = path.with_extension(format!(
            "json.agent-notifier-backup-{}",
            Utc::now().format("%Y%m%d%H%M%S")
        ));
        std::fs::copy(path, &backup)?;
        println!("Backup: {}", backup.display());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?
    }
    config::atomic_write(path, rendered.as_bytes())?;
    println!("Updated: {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn additive_and_idempotent() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("settings.json");
        std::fs::write(&p,r#"{"theme":"dark","hooks":{"Stop":[{"hooks":[{"type":"command","command":"mine"}]}]}}"#).unwrap();
        let bin = Path::new("/x/a");
        edit(&p, "claude", bin, false).unwrap();
        edit(&p, "claude", bin, false).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["hooks"]["Stop"].as_array().unwrap().len(), 2);
        edit(&p, "claude", bin, true).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&std::fs::read_to_string(&p).unwrap()).unwrap()["hooks"]
                ["Stop"]
                .as_array()
                .unwrap()
                .len(),
            1
        )
    }
}
