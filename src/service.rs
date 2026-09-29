use crate::{
    bootstrap, config,
    db::Store,
    model::{self, Agent},
    notify,
};
use serde_json::json;
use std::sync::Mutex;
use zbus::{connection, interface, object_server::SignalEmitter};

pub struct Service {
    store: Mutex<Store>,
}
impl Service {
    pub fn new(mut store: Store) -> Self {
        let _ = bootstrap::import_recent(&mut store);
        Self {
            store: Mutex::new(store),
        }
    }
}

#[interface(name = "io.github.mmmohebi.AgentNotifier1")]
impl Service {
    async fn list_sessions(&self, since_timestamp: i64) -> String {
        match self.store.lock().unwrap().list(since_timestamp) {
            Ok(s) => serde_json::to_string(&s).unwrap_or_else(|_| "[]".into()),
            Err(e) => json!({"error":e.to_string()}).to_string(),
        }
    }
    async fn ingest_event(
        &self,
        source: &str,
        payload: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> String {
        let Some(agent) = Agent::parse(source) else {
            return json!({"ok":false,"error":"unknown source"}).to_string();
        };
        let event = match model::normalize(agent, payload, chrono::Utc::now()) {
            Ok(e) => e,
            Err(e) => return json!({"ok":false,"error":e.to_string()}).to_string(),
        };
        let (changed, notifications_enabled) = {
            let mut store = self.store.lock().unwrap();
            let changed = store.ingest(&event).unwrap_or(false);
            let enabled = store
                .notifications_enabled(&event.session.id)
                .unwrap_or(true);
            (changed, enabled)
        };
        if changed {
            let revision = self.store.lock().unwrap().revision().unwrap_or(0);
            let _ = Self::sessions_changed(&emitter, revision).await;
            if notifications_enabled && event.session.state.should_alert() {
                let cfg = config::load(&config::config_path()).unwrap_or_default();
                let session = event.session.clone();
                tokio::spawn(async move {
                    for error in notify::deliver_all(&cfg, &session).await {
                        eprintln!("notification delivery failed: {error}")
                    }
                });
            }
        }
        json!({"ok":true,"changed":changed}).to_string()
    }
    async fn get_service_status(&self) -> String {
        let (revision, sessions) = {
            let store = self.store.lock().unwrap();
            (
                store.revision().unwrap_or(0),
                store.list(0).map(|items| items.len()).unwrap_or(0),
            )
        };
        let destinations: Vec<serde_json::Value> = config::load(&config::config_path())
            .unwrap_or_default()
            .destinations
            .into_iter()
            .map(|destination| match destination {
                config::Destination::Ntfy {
                    name,
                    enabled,
                    server,
                    topic,
                    token_account,
                } => json!({
                    "name": name,
                    "kind": "ntfy",
                    "enabled": enabled,
                    "endpoint": server,
                    "topic": topic,
                    "has_token": token_account.is_some(),
                }),
                config::Destination::Webhook {
                    name,
                    enabled,
                    url,
                    token_account,
                } => json!({
                    "name": name,
                    "kind": "webhook",
                    "enabled": enabled,
                    "endpoint": url,
                    "topic": "",
                    "has_token": token_account.is_some(),
                }),
            })
            .collect();

        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "revision": revision,
            "sessions": sessions,
            "config": config::config_path(),
            "secret_service": "keyring",
            "notifications_enabled_by_default": self.store.lock().unwrap().global_notifications_enabled().unwrap_or(false),
            "destinations": destinations,
        })
        .to_string()
    }
    async fn set_session_notifications(
        &self,
        session_id: &str,
        enabled: bool,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> String {
        let result = self
            .store
            .lock()
            .unwrap()
            .set_notifications(session_id, enabled);
        match result {
            Ok(true) => {
                let revision = self.store.lock().unwrap().revision().unwrap_or(0);
                let _ = Self::sessions_changed(&emitter, revision).await;
                json!({"ok":true}).to_string()
            }
            Ok(false) => json!({"ok":false,"error":"session not found"}).to_string(),
            Err(e) => json!({"ok":false,"error":e.to_string()}).to_string(),
        }
    }
    async fn set_all_session_notifications(
        &self,
        enabled: bool,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> String {
        let result = self.store.lock().unwrap().set_all_notifications(enabled);
        match result {
            Ok(()) => {
                let revision = self.store.lock().unwrap().revision().unwrap_or(0);
                let _ = Self::sessions_changed(&emitter, revision).await;
                json!({"ok":true,"enabled":enabled}).to_string()
            }
            Err(e) => json!({"ok":false,"error":e.to_string()}).to_string(),
        }
    }
    async fn set_session_hidden(
        &self,
        session_id: &str,
        hidden: bool,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> String {
        let result = self.store.lock().unwrap().set_hidden(session_id, hidden);
        match result {
            Ok(true) => {
                let revision = self.store.lock().unwrap().revision().unwrap_or(0);
                let _ = Self::sessions_changed(&emitter, revision).await;
                json!({"ok":true}).to_string()
            }
            Ok(false) => json!({"ok":false,"error":"session not found"}).to_string(),
            Err(e) => json!({"ok":false,"error":e.to_string()}).to_string(),
        }
    }
    async fn configure_destination(
        &self,
        name: &str,
        kind: &str,
        endpoint: &str,
        topic: &str,
        enabled: bool,
        token: &str,
    ) -> String {
        match configure(name, kind, endpoint, topic, enabled, token) {
            Ok(()) => json!({"ok":true}).to_string(),
            Err(e) => json!({"ok":false,"error":e.to_string()}).to_string(),
        }
    }
    async fn remove_destination(&self, name: &str) -> String {
        match remove(name) {
            Ok(()) => json!({"ok":true}).to_string(),
            Err(e) => json!({"ok":false,"error":e.to_string()}).to_string(),
        }
    }
    async fn test_destination(&self, name: &str) -> String {
        match test(name).await {
            Ok(()) => json!({"ok":true}).to_string(),
            Err(e) => json!({"ok":false,"error":e.to_string()}).to_string(),
        }
    }
    #[zbus(signal)]
    async fn sessions_changed(emitter: &SignalEmitter<'_>, revision: u64) -> zbus::Result<()>;
}

pub async fn run() -> anyhow::Result<()> {
    let data = config::data_dir();
    std::fs::create_dir_all(&data)?;
    let store = Store::open(&data.join("sessions.db"))?;
    store.prune()?;
    let service = Service::new(store);
    let _conn = connection::Builder::session()?
        .name("io.github.mmmohebi.AgentNotifier")?
        .serve_at("/io/github/mmmohebi/AgentNotifier", service)?
        .build()
        .await?;
    recover_spool().await;
    tokio::signal::ctrl_c().await?;
    Ok(())
}
async fn recover_spool() {
    let dir = config::data_dir().join("spool");
    let Ok(items) = std::fs::read_dir(dir) else {
        return;
    };
    let Ok(c) = zbus::Connection::session().await else {
        return;
    };
    let Ok(p) = zbus::Proxy::new(
        &c,
        "io.github.mmmohebi.AgentNotifier",
        "/io/github/mmmohebi/AgentNotifier",
        "io.github.mmmohebi.AgentNotifier1",
    )
    .await
    else {
        return;
    };
    for f in items.flatten() {
        let Ok(bytes) = std::fs::read(f.path()) else {
            continue;
        };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        let (Some(s), Some(payload)) = (v["source"].as_str(), v["payload"].as_str()) else {
            continue;
        };
        if p.call::<_, _, String>("IngestEvent", &(s, payload))
            .await
            .is_ok()
        {
            let _ = std::fs::remove_file(f.path());
        }
    }
}
pub fn configure(
    name: &str,
    kind: &str,
    endpoint: &str,
    topic: &str,
    enabled: bool,
    token: &str,
) -> anyhow::Result<()> {
    anyhow::ensure!(!name.trim().is_empty(), "destination name is required");
    anyhow::ensure!(
        !endpoint.trim().is_empty(),
        "server or webhook URL is required"
    );
    anyhow::ensure!(
        matches!(kind, "ntfy" | "webhook"),
        "kind must be ntfy or webhook"
    );
    if kind == "ntfy" {
        anyhow::ensure!(!topic.trim().is_empty(), "ntfy topic is required");
    }

    let path = config::config_path();
    let mut cfg = config::load(&path)?;
    let existing_account = cfg
        .destinations
        .iter()
        .find(|destination| destination.name() == name)
        .and_then(|destination| match destination {
            config::Destination::Ntfy { token_account, .. }
            | config::Destination::Webhook { token_account, .. } => token_account.clone(),
        });
    cfg.destinations.retain(|d| d.name() != name);
    let account = if token.is_empty() {
        existing_account
    } else {
        let a = format!("destination:{name}");
        config::secret_set(&a, token)?;
        Some(a)
    };
    let d = match kind {
        "ntfy" => config::Destination::Ntfy {
            name: name.into(),
            enabled,
            server: endpoint.into(),
            topic: topic.into(),
            token_account: account,
        },
        "webhook" => config::Destination::Webhook {
            name: name.into(),
            enabled,
            url: endpoint.into(),
            token_account: account,
        },
        _ => unreachable!("destination kind was validated above"),
    };
    cfg.destinations.push(d);
    config::save(&path, &cfg)
}
pub fn remove(name: &str) -> anyhow::Result<()> {
    let p = config::config_path();
    let mut c = config::load(&p)?;
    for d in c.destinations.iter().filter(|d| d.name() == name) {
        match d {
            config::Destination::Ntfy { token_account, .. }
            | config::Destination::Webhook { token_account, .. } => {
                if let Some(a) = token_account {
                    config::secret_delete(a)
                }
            }
        }
    }
    c.destinations.retain(|d| d.name() != name);
    config::save(&p, &c)
}
pub async fn test(name: &str) -> anyhow::Result<()> {
    let c = config::load(&config::config_path())?;
    let d = c
        .destinations
        .iter()
        .find(|d| d.name() == name)
        .ok_or_else(|| anyhow::anyhow!("destination not found"))?;
    let now = chrono::Utc::now();
    let s = model::Session {
        id: "test".into(),
        agent: Agent::Codex,
        title: "Test notification".into(),
        project: "agent-notifier".into(),
        state: model::State::NeedsAttention,
        preview: "Agent Notifier is configured correctly.".into(),
        created_at: now,
        updated_at: now,
        live: true,
        notifications_enabled: true,
        hidden: false,
    };
    notify::deliver(d, &s).await
}
