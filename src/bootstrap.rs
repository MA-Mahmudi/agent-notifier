use crate::{
    db::Store,
    model::{redact, Agent, NormalizedEvent, Session, State},
};
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub fn import_recent(store: &mut Store) -> anyhow::Result<usize> {
    let mut n = 0;
    for (agent, root) in roots() {
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root)
            .max_depth(8)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_type().is_file()
                    && (e
                        .path()
                        .extension()
                        .is_some_and(|x| x == "json" || x == "jsonl"))
            })
        {
            if entry
                .metadata()?
                .modified()?
                .elapsed()
                .map(|x| x.as_secs() > 86400)
                .unwrap_or(true)
            {
                continue;
            }
            if let Some(e) = read_metadata(agent.clone(), entry.path()) {
                if store.ingest(&e)? {
                    n += 1
                }
            }
        }
    }
    Ok(n)
}
fn roots() -> Vec<(Agent, PathBuf)> {
    let h = dirs::home_dir().unwrap_or_default();
    vec![
        (Agent::Codex, h.join(".codex/sessions")),
        (Agent::Codex, h.join(".codex/session_index.json")),
        (Agent::Claude, h.join(".claude/projects")),
        (Agent::Claude, h.join(".claude/history.jsonl")),
    ]
}
fn read_metadata(agent: Agent, path: &Path) -> Option<NormalizedEvent> {
    let text = std::fs::read_to_string(path).ok()?;
    let value = text
        .lines()
        .filter_map(|x| serde_json::from_str::<Value>(x).ok())
        .next_back()
        .or_else(|| serde_json::from_str(&text).ok())?;
    let id = value
        .get("session_id")
        .or_else(|| value.get("sessionId"))
        .and_then(Value::as_str)
        .or_else(|| path.file_stem()?.to_str())?;
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let ts: DateTime<Utc> = modified.into();
    if ts < Utc::now() - Duration::hours(24) {
        return None;
    }
    let cwd = value.get("cwd").and_then(Value::as_str).unwrap_or("");
    let project = Path::new(cwd)
        .file_name()
        .and_then(|x| x.to_str())
        .unwrap_or(cwd);
    let preview = value
        .get("message")
        .and_then(|m| {
            m.as_str()
                .or_else(|| m.get("content").and_then(Value::as_str))
        })
        .or_else(|| value.get("prompt").and_then(Value::as_str))
        .unwrap_or("");
    let session = Session {
        id: format!("{}:{id}", agent.as_str()),
        agent,
        title: if preview.is_empty() {
            "Imported session".into()
        } else {
            redact(preview)
        },
        project: redact(project),
        state: State::Unknown,
        preview: redact(preview),
        created_at: ts,
        updated_at: ts,
        live: false,
        notifications_enabled: false,
        hidden: false,
    };
    Some(NormalizedEvent {
        event_id: format!("bootstrap:{}:{}", session.agent.as_str(), id),
        session,
        starts_session: false,
    })
}
