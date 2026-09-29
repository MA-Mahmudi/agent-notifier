use crate::model::{Agent, NormalizedEvent, Session, State};
use anyhow::Context;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS sessions(id TEXT PRIMARY KEY, agent TEXT NOT NULL, title TEXT NOT NULL, project TEXT NOT NULL, state TEXT NOT NULL, preview TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, live INTEGER NOT NULL, notifications_enabled INTEGER NOT NULL DEFAULT 0, hidden INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS events(event_id TEXT PRIMARY KEY, received_at INTEGER NOT NULL); CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value INTEGER NOT NULL);")?;
        if !has_column(&conn, "sessions", "notifications_enabled")? {
            conn.execute(
                "ALTER TABLE sessions ADD COLUMN notifications_enabled INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !has_column(&conn, "sessions", "hidden")? {
            conn.execute(
                "ALTER TABLE sessions ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        let has_notification_default = conn
            .query_row(
                "SELECT 1 FROM meta WHERE key='notifications_enabled'",
                [],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !has_notification_default {
            conn.execute("UPDATE sessions SET notifications_enabled=0", [])?;
            conn.execute(
                "INSERT INTO meta(key,value) VALUES('notifications_enabled',0)",
                [],
            )?;
        }
        Ok(Self { conn })
    }

    pub fn ingest(&mut self, e: &NormalizedEvent) -> anyhow::Result<bool> {
        let tx = self.conn.transaction()?;
        if tx
            .query_row(
                "SELECT 1 FROM events WHERE event_id=?1",
                [&e.event_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            return Ok(false);
        }
        let current: Option<i64> = tx
            .query_row(
                "SELECT updated_at FROM sessions WHERE id=?1",
                [&e.session.id],
                |r| r.get(0),
            )
            .optional()?;
        if current.is_some_and(|x| x > e.session.updated_at.timestamp()) {
            tx.execute(
                "INSERT INTO events VALUES(?1,?2)",
                params![e.event_id, Utc::now().timestamp()],
            )?;
            tx.commit()?;
            return Ok(false);
        }
        if e.starts_session {
            let changed = if current.is_some() {
                tx.execute(
                    "UPDATE sessions SET hidden=0 WHERE id=?1 AND hidden=1",
                    [&e.session.id],
                )?
            } else {
                0
            };
            tx.execute(
                "INSERT INTO events VALUES(?1,?2)",
                params![e.event_id, Utc::now().timestamp()],
            )?;
            if changed > 0 {
                bump_revision(&tx)?;
            }
            tx.commit()?;
            return Ok(changed > 0);
        }
        if current.is_none() && !e.has_prompt {
            tx.execute(
                "INSERT INTO events VALUES(?1,?2)",
                params![e.event_id, Utc::now().timestamp()],
            )?;
            tx.commit()?;
            return Ok(false);
        }
        if current.is_some() && !e.session.live {
            tx.execute(
                "INSERT INTO events VALUES(?1,?2)",
                params![e.event_id, Utc::now().timestamp()],
            )?;
            tx.commit()?;
            return Ok(false);
        }
        let s = &e.session;
        let notifications_enabled = tx
            .query_row(
                "SELECT value FROM meta WHERE key='notifications_enabled'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0)
            != 0;
        tx.execute("INSERT INTO sessions(id,agent,title,project,state,preview,created_at,updated_at,live,notifications_enabled,hidden) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(id) DO UPDATE SET title=CASE WHEN excluded.title='Agent session' THEN sessions.title ELSE excluded.title END,project=CASE WHEN excluded.project='' THEN sessions.project ELSE excluded.project END,state=excluded.state,preview=CASE WHEN excluded.preview='' THEN sessions.preview ELSE excluded.preview END,updated_at=excluded.updated_at,live=1", params![s.id,s.agent.as_str(),s.title,s.project,s.state.as_str(),s.preview,s.created_at.timestamp(),s.updated_at.timestamp(),s.live,notifications_enabled,s.hidden])?;
        tx.execute(
            "INSERT INTO events VALUES(?1,?2)",
            params![e.event_id, Utc::now().timestamp()],
        )?;
        tx.execute(
            "INSERT INTO meta VALUES('revision',1) ON CONFLICT(key) DO UPDATE SET value=value+1",
            [],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn list(&self, since: i64) -> anyhow::Result<Vec<Session>> {
        let cutoff = since.max((Utc::now() - Duration::hours(24)).timestamp());
        let mut q = self.conn.prepare("SELECT id,agent,title,project,state,preview,created_at,updated_at,live,notifications_enabled,hidden FROM sessions WHERE updated_at>=?1 ORDER BY CASE state WHEN 'needs_attention' THEN 0 WHEN 'working' THEN 1 ELSE 2 END, updated_at DESC")?;
        let rows = q.query_map([cutoff], |r| {
            Ok(Session {
                id: r.get(0)?,
                agent: parse_agent(r.get::<_, String>(1)?),
                title: r.get(2)?,
                project: r.get(3)?,
                state: parse_state(r.get::<_, String>(4)?),
                preview: r.get(5)?,
                created_at: from_ts(r.get(6)?),
                updated_at: from_ts(r.get(7)?),
                live: r.get::<_, i64>(8)? != 0,
                notifications_enabled: r.get::<_, i64>(9)? != 0,
                hidden: r.get::<_, i64>(10)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().context("read sessions")
    }
    pub fn prune(&self) -> anyhow::Result<usize> {
        let cutoff = (Utc::now() - Duration::hours(24)).timestamp();
        let sessions = self
            .conn
            .execute("DELETE FROM sessions WHERE updated_at<?1", [cutoff])?;
        self.conn
            .execute("DELETE FROM events WHERE received_at<?1", [cutoff])?;
        Ok(sessions)
    }
    pub fn purge_promptless_sessions(&mut self) -> anyhow::Result<usize> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM events WHERE event_id IN (SELECT 'bootstrap:' || agent || ':' || substr(id, instr(id, ':') + 1) FROM sessions WHERE state='unknown' AND trim(preview)='')",
            [],
        )?;
        let removed = tx.execute(
            "DELETE FROM sessions WHERE state='unknown' AND trim(preview)=''",
            [],
        )?;
        if removed > 0 {
            bump_revision(&tx)?;
        }
        tx.commit()?;
        Ok(removed)
    }
    pub fn revision(&self) -> anyhow::Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key='revision'", [], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or(0))
    }
    pub fn notifications_enabled(&self, session_id: &str) -> anyhow::Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT notifications_enabled FROM sessions WHERE id=?1",
                [session_id],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0)
            != 0)
    }
    pub fn global_notifications_enabled(&self) -> anyhow::Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key='notifications_enabled'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0)
            != 0)
    }
    pub fn set_notifications(&self, session_id: &str, enabled: bool) -> anyhow::Result<bool> {
        let changed = self.conn.execute(
            "UPDATE sessions SET notifications_enabled=?2 WHERE id=?1",
            params![session_id, enabled],
        )?;
        if changed > 0 {
            self.conn.execute(
                "INSERT INTO meta VALUES('revision',1) ON CONFLICT(key) DO UPDATE SET value=value+1",
                [],
            )?;
        }
        Ok(changed > 0)
    }
    pub fn set_all_notifications(&mut self, enabled: bool) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE sessions SET notifications_enabled=?1",
            [enabled as i64],
        )?;
        tx.execute(
            "INSERT INTO meta(key,value) VALUES('notifications_enabled',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [enabled as i64],
        )?;
        bump_revision(&tx)?;
        tx.commit()?;
        Ok(())
    }
    pub fn set_hidden(&self, session_id: &str, hidden: bool) -> anyhow::Result<bool> {
        let changed = self.conn.execute(
            "UPDATE sessions SET hidden=?2 WHERE id=?1",
            params![session_id, hidden],
        )?;
        if changed > 0 {
            self.conn.execute(
                "INSERT INTO meta VALUES('revision',1) ON CONFLICT(key) DO UPDATE SET value=value+1",
                [],
            )?;
        }
        Ok(changed > 0)
    }
}

fn bump_revision(tx: &rusqlite::Transaction<'_>) -> anyhow::Result<()> {
    tx.execute(
        "INSERT INTO meta VALUES('revision',1) ON CONFLICT(key) DO UPDATE SET value=value+1",
        [],
    )?;
    Ok(())
}
fn has_column(conn: &Connection, table: &str, column: &str) -> anyhow::Result<bool> {
    let mut query = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = query.query_map([], |row| row.get::<_, String>(1))?;
    for name in names {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}
fn from_ts(v: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(v, 0).unwrap_or(DateTime::UNIX_EPOCH)
}
fn parse_agent(s: String) -> Agent {
    if s == "claude" {
        Agent::Claude
    } else {
        Agent::Codex
    }
}
fn parse_state(s: String) -> State {
    match s.as_str() {
        "working" => State::Working,
        "needs_attention" => State::NeedsAttention,
        "completed" => State::Completed,
        "failed" => State::Failed,
        "ended" => State::Ended,
        _ => State::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::normalize;
    #[test]
    fn dedupes_and_filters_stale() {
        let d = tempfile::tempdir().unwrap();
        let mut s = Store::open(&d.path().join("a.db")).unwrap();
        let now = Utc::now();
        let e = normalize(
            Agent::Codex,
            r#"{"session_id":"1","hook_event_name":"UserPromptSubmit","prompt":"first"}"#,
            now,
        )
        .unwrap();
        assert!(s.ingest(&e).unwrap());
        assert!(!s.ingest(&e).unwrap());
        assert_eq!(s.list(0).unwrap().len(), 1);
        assert!(!s.notifications_enabled("codex:1").unwrap());
        assert!(s.set_notifications("codex:1", true).unwrap());
        assert!(s.notifications_enabled("codex:1").unwrap());
        let next = normalize(
            Agent::Codex,
            r#"{"session_id":"1","hook_event_name":"UserPromptSubmit","prompt":"again"}"#,
            now,
        )
        .unwrap();
        assert!(s.ingest(&next).unwrap());
        assert!(s.notifications_enabled("codex:1").unwrap());
        assert!(s.set_hidden("codex:1", true).unwrap());
        assert!(s.list(0).unwrap()[0].hidden);
        assert!(s.set_hidden("codex:1", false).unwrap());
        assert!(!s.list(0).unwrap()[0].hidden);
    }

    #[test]
    fn ignores_new_sessions_until_the_first_prompt() {
        let d = tempfile::tempdir().unwrap();
        let mut store = Store::open(&d.path().join("a.db")).unwrap();
        let now = Utc::now();

        let start = normalize(
            Agent::Codex,
            r#"{"session_id":"1","hook_event_name":"SessionStart"}"#,
            now,
        )
        .unwrap();
        assert!(!store.ingest(&start).unwrap());
        assert!(store.list(0).unwrap().is_empty());

        let stop = normalize(
            Agent::Codex,
            r#"{"session_id":"1","hook_event_name":"Stop"}"#,
            now + Duration::seconds(1),
        )
        .unwrap();
        assert!(!store.ingest(&stop).unwrap());
        assert!(store.list(0).unwrap().is_empty());

        let prompt = normalize(
            Agent::Codex,
            r#"{"session_id":"1","hook_event_name":"UserPromptSubmit","prompt":"hello"}"#,
            now + Duration::seconds(2),
        )
        .unwrap();
        assert!(store.ingest(&prompt).unwrap());
        assert_eq!(store.list(0).unwrap().len(), 1);
    }

    #[test]
    fn purges_existing_promptless_unknown_sessions() {
        let d = tempfile::tempdir().unwrap();
        let mut store = Store::open(&d.path().join("a.db")).unwrap();
        store.conn.execute(
            "INSERT INTO sessions VALUES('codex:junk','codex','Imported session','','unknown','',0,?1,0,0,0)",
            [Utc::now().timestamp()],
        ).unwrap();
        store
            .conn
            .execute(
                "INSERT INTO events VALUES('bootstrap:codex:junk',?1)",
                [Utc::now().timestamp()],
            )
            .unwrap();

        assert_eq!(store.purge_promptless_sessions().unwrap(), 1);
        assert!(store.list(0).unwrap().is_empty());
        assert!(store
            .conn
            .query_row(
                "SELECT 1 FROM events WHERE event_id='bootstrap:codex:junk'",
                [],
                |_| Ok(()),
            )
            .optional()
            .unwrap()
            .is_none());
    }

    #[test]
    fn global_notification_setting_applies_to_existing_and_new_sessions() {
        let d = tempfile::tempdir().unwrap();
        let mut store = Store::open(&d.path().join("a.db")).unwrap();
        let first = normalize(
            Agent::Codex,
            r#"{"session_id":"1","hook_event_name":"UserPromptSubmit","prompt":"first"}"#,
            Utc::now(),
        )
        .unwrap();
        store.ingest(&first).unwrap();
        assert!(!store.global_notifications_enabled().unwrap());
        assert!(!store.notifications_enabled("codex:1").unwrap());

        store.set_all_notifications(true).unwrap();
        assert!(store.global_notifications_enabled().unwrap());
        assert!(store.notifications_enabled("codex:1").unwrap());

        let second = normalize(
            Agent::Codex,
            r#"{"session_id":"2","hook_event_name":"UserPromptSubmit","prompt":"second"}"#,
            Utc::now(),
        )
        .unwrap();
        store.ingest(&second).unwrap();
        assert!(store.notifications_enabled("codex:2").unwrap());

        store.set_all_notifications(false).unwrap();
        assert!(!store.global_notifications_enabled().unwrap());
        assert!(!store.notifications_enabled("codex:1").unwrap());
        assert!(!store.notifications_enabled("codex:2").unwrap());
    }

    #[test]
    fn resumed_session_is_visible_but_same_run_activity_stays_hidden() {
        let d = tempfile::tempdir().unwrap();
        let mut store = Store::open(&d.path().join("a.db")).unwrap();
        let now = Utc::now();

        let initial = normalize(
            Agent::Codex,
            r#"{"session_id":"1","hook_event_name":"UserPromptSubmit","prompt":"first"}"#,
            now,
        )
        .unwrap();
        store.ingest(&initial).unwrap();
        store.set_hidden("codex:1", true).unwrap();

        let same_run = normalize(
            Agent::Codex,
            r#"{"session_id":"1","hook_event_name":"UserPromptSubmit","prompt":"again"}"#,
            now + Duration::seconds(1),
        )
        .unwrap();
        store.ingest(&same_run).unwrap();
        assert!(store.list(0).unwrap()[0].hidden);

        let resumed = normalize(
            Agent::Codex,
            r#"{"session_id":"1","hook_event_name":"SessionStart"}"#,
            now + Duration::seconds(2),
        )
        .unwrap();
        store.ingest(&resumed).unwrap();
        assert!(!store.list(0).unwrap()[0].hidden);
    }

    #[test]
    fn post_tool_use_clears_attention_during_the_same_turn() {
        let d = tempfile::tempdir().unwrap();
        let mut store = Store::open(&d.path().join("a.db")).unwrap();
        let now = Utc::now();

        for (offset, event) in [
            (
                0,
                r#"{"session_id":"1","hook_event_name":"UserPromptSubmit","prompt":"run it"}"#,
            ),
            (
                1,
                r#"{"session_id":"1","hook_event_name":"PermissionRequest","tool_name":"Bash"}"#,
            ),
            (
                2,
                r#"{"session_id":"1","hook_event_name":"PostToolUse","tool_name":"Bash"}"#,
            ),
        ] {
            let normalized =
                normalize(Agent::Codex, event, now + Duration::seconds(offset)).unwrap();
            store.ingest(&normalized).unwrap();
        }

        let session = &store.list(0).unwrap()[0];
        assert_eq!(session.state, State::Working);
        assert_eq!(session.title, "run it");
        assert_eq!(session.preview, "run it");
    }

    #[test]
    fn migrates_existing_sessions_with_notifications_enabled() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("old.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE sessions(id TEXT PRIMARY KEY, agent TEXT NOT NULL, title TEXT NOT NULL, project TEXT NOT NULL, state TEXT NOT NULL, preview TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, live INTEGER NOT NULL, notifications_enabled INTEGER NOT NULL DEFAULT 1); CREATE TABLE events(event_id TEXT PRIMARY KEY, received_at INTEGER NOT NULL); CREATE TABLE meta(key TEXT PRIMARY KEY, value INTEGER NOT NULL); INSERT INTO sessions VALUES('codex:old','codex','Old','project','completed','done',0,0,1,1);").unwrap();
        drop(conn);

        let store = Store::open(&path).unwrap();
        assert!(has_column(&store.conn, "sessions", "notifications_enabled").unwrap());
        assert!(has_column(&store.conn, "sessions", "hidden").unwrap());
        assert!(!store.global_notifications_enabled().unwrap());
        assert!(!store.notifications_enabled("codex:old").unwrap());
    }
}
