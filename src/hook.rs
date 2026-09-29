use crate::{config, model::Agent};
use anyhow::Context;
use std::io::Read;
use zbus::blocking::{Connection, Proxy};

pub fn run(agent: Agent) -> anyhow::Result<()> {
    let mut payload = String::new();
    std::io::stdin().read_to_string(&mut payload)?;
    if serde_json::from_str::<serde_json::Value>(&payload).is_err() {
        return Ok(());
    }
    if send(agent.as_str(), &payload).is_err() {
        spool(agent.as_str(), &payload)?
    }
    Ok(())
}
fn send(source: &str, payload: &str) -> anyhow::Result<()> {
    let c = Connection::session()?;
    let p = Proxy::new(
        &c,
        "io.github.mmmohebi.AgentNotifier",
        "/io/github/mmmohebi/AgentNotifier",
        "io.github.mmmohebi.AgentNotifier1",
    )?;
    let _: String = p.call("IngestEvent", &(source, payload))?;
    Ok(())
}
fn spool(source: &str, payload: &str) -> anyhow::Result<()> {
    let dir = config::data_dir().join("spool");
    std::fs::create_dir_all(&dir)?;
    let value = serde_json::json!({"source":source,"payload":payload});
    let dest = dir.join(format!("{}.json", uuid::Uuid::new_v4()));
    config::atomic_write(&dest, serde_json::to_vec(&value)?.as_slice()).context("spool hook event")
}
