use crate::{
    config::{self, Config, Destination},
    model::Session,
};
use anyhow::Context;
use reqwest::{Client, StatusCode};
use serde_json::json;
use std::time::Duration;

pub async fn deliver_all(config: &Config, session: &Session) -> Vec<String> {
    let mut errors = vec![];
    for d in config.destinations.iter().filter(|d| d.enabled()) {
        if let Err(e) = deliver(d, session).await {
            errors.push(format!("{}: {e:#}", d.name()));
        }
    }
    errors
}
pub async fn deliver(d: &Destination, s: &Session) -> anyhow::Result<()> {
    let client = Client::builder().timeout(Duration::from_secs(10)).build()?;
    let token = match d {
        Destination::Ntfy { token_account, .. } | Destination::Webhook { token_account, .. } => {
            token_account
                .as_deref()
                .map(config::secret_get)
                .transpose()?
        }
    };
    let mut delay = Duration::from_millis(250);
    for attempt in 0..4 {
        let req = match d {
            Destination::Ntfy { server, topic, .. } => client
                .post(format!("{}/{}", server.trim_end_matches('/'), topic))
                .header(
                    "Title",
                    format!("{}: {}", s.agent.as_str(), s.state.as_str()),
                )
                .header(
                    "Priority",
                    if s.state.as_str() == "needs_attention" {
                        "high"
                    } else {
                        "default"
                    },
                )
                .body(format!("{} · {}\n{}", s.project, s.title, s.preview)),
            Destination::Webhook { url, .. } => client
                .post(url)
                .json(&json!({"version":1,"type":"session.state_changed","session":s})),
        };
        let req = if let Some(ref t) = token {
            req.bearer_auth(t)
        } else {
            req
        };
        match req.send().await {
            Ok(r) if r.status().is_success() => return Ok(()),
            Ok(r) if !transient(r.status()) => anyhow::bail!("HTTP {}", r.status()),
            Ok(r) => {
                if attempt == 3 {
                    anyhow::bail!("HTTP {} after retries", r.status())
                }
            }
            Err(e) => {
                if attempt == 3 {
                    return Err(e).context("request failed after retries");
                }
            }
        }
        tokio::time::sleep(delay).await;
        delay *= 2;
    }
    unreachable!()
}
fn transient(s: StatusCode) -> bool {
    s == StatusCode::TOO_MANY_REQUESTS || s.is_server_error()
}
