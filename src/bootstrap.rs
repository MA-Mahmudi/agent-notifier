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
    let values: Vec<Value> = text
        .lines()
        .filter_map(|x| serde_json::from_str::<Value>(x).ok())
        .collect();
    let values = if values.is_empty() {
        vec![serde_json::from_str(&text).ok()?]
    } else {
        values
    };
    let id = values.iter().find_map(session_id)?;
    let prompt = values.iter().filter_map(prompt_text).next_back()?;
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let ts: DateTime<Utc> = modified.into();
    if ts < Utc::now() - Duration::hours(24) {
        return None;
    }
    let cwd = values.iter().find_map(cwd).unwrap_or("");
    let project = Path::new(cwd)
        .file_name()
        .and_then(|x| x.to_str())
        .unwrap_or(cwd);
    let preview = redact(&prompt);
    let session = Session {
        id: format!("{}:{id}", agent.as_str()),
        agent,
        title: preview.clone(),
        project: redact(project),
        state: State::Unknown,
        preview,
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
        has_prompt: true,
    })
}

fn session_id(value: &Value) -> Option<&str> {
    value
        .get("session_id")
        .or_else(|| value.get("sessionId"))
        .or_else(|| value.get("conversation_id"))
        .and_then(Value::as_str)
        .or_else(|| {
            (value.get("type").and_then(Value::as_str) == Some("session_meta"))
                .then(|| {
                    value
                        .pointer("/payload/session_id")
                        .or_else(|| value.pointer("/payload/id"))
                        .and_then(Value::as_str)
                })
                .flatten()
        })
}

fn cwd(value: &Value) -> Option<&str> {
    value
        .get("cwd")
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/payload/cwd").and_then(Value::as_str))
        .filter(|cwd| !cwd.is_empty())
}

fn prompt_text(value: &Value) -> Option<String> {
    if let Some(prompt) = value.get("prompt").and_then(Value::as_str) {
        return non_empty(prompt.to_string());
    }

    let is_codex_prompt = value.get("type").and_then(Value::as_str) == Some("response_item")
        && value.pointer("/payload/type").and_then(Value::as_str) == Some("message")
        && value.pointer("/payload/role").and_then(Value::as_str) == Some("user");
    if is_codex_prompt {
        return content_text(value.pointer("/payload/content")?);
    }

    let is_claude_prompt = value.get("type").and_then(Value::as_str) == Some("user")
        && !value
            .get("isMeta")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    if is_claude_prompt {
        return content_text(value.pointer("/message/content")?);
    }

    None
}

fn content_text(content: &Value) -> Option<String> {
    match content {
        Value::String(text) => non_empty(text.clone()),
        Value::Array(items) => non_empty(
            items
                .iter()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => None,
    }
}

fn non_empty(text: String) -> Option<String> {
    (!text.trim().is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_codex_transcript_without_user_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"type":"session_meta","payload":{"id":"abc","cwd":"/tmp/project"}}"#,
                "\n",
                r#"{"type":"event_msg","payload":{"type":"task_complete"}}"#,
            ),
        )
        .unwrap();

        assert!(read_metadata(Agent::Codex, &path).is_none());
    }

    #[test]
    fn imports_codex_transcript_with_real_id_and_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"type":"session_meta","payload":{"id":"abc","cwd":"/tmp/project"}}"#,
                "\n",
                r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Run the tests"}]}}"#,
            ),
        )
        .unwrap();

        let event = read_metadata(Agent::Codex, &path).unwrap();
        assert_eq!(event.session.id, "codex:abc");
        assert_eq!(event.session.project, "project");
        assert_eq!(event.session.preview, "Run the tests");
        assert!(event.has_prompt);
    }

    #[test]
    fn imports_claude_user_prompt_but_ignores_metadata_messages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"type":"user","sessionId":"abc","isMeta":true,"message":{"role":"user","content":"internal"}}"#,
                "\n",
                r#"{"type":"user","sessionId":"abc","cwd":"/tmp/project","message":{"role":"user","content":"Fix the bug"}}"#,
            ),
        )
        .unwrap();

        let event = read_metadata(Agent::Claude, &path).unwrap();
        assert_eq!(event.session.id, "claude:abc");
        assert_eq!(event.session.preview, "Fix the bug");
    }
}
