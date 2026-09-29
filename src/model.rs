use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Agent {
    Codex,
    Claude,
}

impl Agent {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "codex" => Some(Self::Codex),
            "claude" => Some(Self::Claude),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Unknown,
    Working,
    NeedsAttention,
    Completed,
    Failed,
    Ended,
}

impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Working => "working",
            Self::NeedsAttention => "needs_attention",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Ended => "ended",
        }
    }
    pub fn should_alert(&self) -> bool {
        matches!(self, Self::NeedsAttention | Self::Completed | Self::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub agent: Agent,
    pub title: String,
    pub project: String,
    pub state: State,
    pub preview: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub live: bool,
    pub notifications_enabled: bool,
    pub hidden: bool,
}

#[derive(Debug, Clone)]
pub struct NormalizedEvent {
    pub session: Session,
    pub event_id: String,
}

fn first_str<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| v.get(*k).and_then(Value::as_str))
}

pub fn redact(input: &str) -> String {
    let ansi = Regex::new(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07]*(?:\x07|\x1b\\))").unwrap();
    let kv = Regex::new(
        r"(?i)\b(authorization|bearer|api[_-]?key|password|token|secret)\b\s*[:=]\s*([^\s,;]+)",
    )
    .unwrap();
    let bearer = Regex::new(r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{8,}").unwrap();
    let compact = Regex::new(r"\s+").unwrap();
    let s = ansi.replace_all(input, "");
    let s = bearer.replace_all(&s, "Bearer [REDACTED]");
    let s = kv.replace_all(&s, "$1=[REDACTED]");
    compact
        .replace_all(s.trim(), " ")
        .chars()
        .take(500)
        .collect()
}

pub fn normalize(
    agent: Agent,
    payload: &str,
    now: DateTime<Utc>,
) -> anyhow::Result<NormalizedEvent> {
    let v: Value = serde_json::from_str(payload)?;
    let event = first_str(&v, &["hook_event_name", "event", "type"]).unwrap_or("unknown");
    let id = first_str(&v, &["session_id", "sessionId", "conversation_id"])
        .ok_or_else(|| anyhow::anyhow!("hook payload has no session id"))?;
    let cwd = first_str(&v, &["cwd", "project_dir"]).unwrap_or("");
    let project = Path::new(cwd)
        .file_name()
        .and_then(|x| x.to_str())
        .unwrap_or(cwd);
    let preview = first_str(
        &v,
        &[
            "prompt",
            "message",
            "last_assistant_message",
            "error",
            "reason",
        ],
    )
    .or_else(|| v.pointer("/notification/message").and_then(Value::as_str))
    .unwrap_or("");
    let state = match event.to_ascii_lowercase().as_str() {
        "sessionstart" => State::Unknown,
        "userpromptsubmit" => State::Working,
        "permissionrequest" | "notification" => State::NeedsAttention,
        "stopfailure" => State::Failed,
        "interrupt" | "sessionend" => State::Ended,
        "stop" => {
            if v.get("background_work")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                State::Working
            } else {
                State::Completed
            }
        }
        _ => State::Unknown,
    };
    let title = first_str(&v, &["title", "session_title"]).unwrap_or({
        if preview.is_empty() {
            "Agent session"
        } else {
            preview
        }
    });
    let ts = first_str(&v, &["timestamp", "created_at"])
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|x| x.with_timezone(&Utc))
        .unwrap_or(now);
    let mut hasher = DefaultHasher::new();
    payload.hash(&mut hasher);
    let raw_key = format!("{}:{id}:{event}:{:x}", agent.as_str(), hasher.finish());
    Ok(NormalizedEvent {
        event_id: raw_key,
        session: Session {
            id: format!("{}:{id}", agent.as_str()),
            agent,
            title: redact(title),
            project: redact(project),
            state,
            preview: redact(preview),
            created_at: ts,
            updated_at: ts,
            live: true,
            notifications_enabled: false,
            hidden: false,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_secrets_and_ansi() {
        assert_eq!(
            redact("\x1b[31mhi\x1b[0m token=abc123456 password: nope"),
            "hi token=[REDACTED] password=[REDACTED]"
        );
    }
    #[test]
    fn maps_stop_failure() {
        let e = normalize(
            Agent::Claude,
            r#"{"session_id":"x","hook_event_name":"StopFailure","cwd":"/tmp/a","error":"bad"}"#,
            Utc::now(),
        )
        .unwrap();
        assert_eq!(e.session.state, State::Failed);
    }
    #[test]
    fn stop_with_background_stays_working() {
        let e = normalize(
            Agent::Codex,
            r#"{"session_id":"x","hook_event_name":"Stop","background_work":true}"#,
            Utc::now(),
        )
        .unwrap();
        assert_eq!(e.session.state, State::Working);
    }
}
