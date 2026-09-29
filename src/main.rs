mod bootstrap;
mod config;
mod db;
mod hook;
mod model;
mod notify;
mod service;
mod setup;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "agent-notifier",
    version,
    about = "Monitor local Codex and Claude Code sessions"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Daemon,
    Hook {
        #[arg(value_parser=["codex","claude"])]
        source: String,
    },
    Setup {
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        binary: Option<PathBuf>,
    },
    Uninstall {
        #[arg(long)]
        binary: Option<PathBuf>,
    },
    Configure {
        name: String,
        #[arg(long,value_parser=["ntfy","webhook"])]
        kind: String,
        #[arg(long)]
        endpoint: String,
        #[arg(long, default_value = "")]
        topic: String,
        #[arg(long, default_value = "")]
        token: String,
        #[arg(long,default_value_t=true,action=clap::ArgAction::Set)]
        enabled: bool,
    },
    RemoveDestination {
        name: String,
    },
    Doctor,
    TestNotification {
        name: String,
    },
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Daemon => service::run().await?,
        Command::Hook { source } => {
            if let Some(agent) = model::Agent::parse(&source) {
                match tokio::task::spawn_blocking(move || hook::run(agent)).await {
                    Ok(Err(e)) => eprintln!("agent-notifier: {e:#}"),
                    Err(e) => eprintln!("agent-notifier: hook worker failed: {e}"),
                    Ok(Ok(())) => {}
                }
            }
        }
        Command::Setup { apply, binary } => {
            setup::setup(apply, &binary.unwrap_or(std::env::current_exe()?))?
        }
        Command::Uninstall { binary } => {
            setup::uninstall(&binary.unwrap_or(std::env::current_exe()?))?
        }
        Command::Configure {
            name,
            kind,
            endpoint,
            topic,
            token,
            enabled,
        } => {
            service::configure(&name, &kind, &endpoint, &topic, enabled, &token)?;
            println!("Configured {name}")
        }
        Command::RemoveDestination { name } => {
            service::remove(&name)?;
            println!("Removed {name}")
        }
        Command::Doctor => {
            if let Err(e) = tokio::task::spawn_blocking(doctor).await {
                eprintln!("doctor failed: {e}");
            }
        }
        Command::TestNotification { name } => {
            service::test(&name).await?;
            println!("Notification delivered")
        }
    }
    Ok(())
}
fn doctor() {
    println!("Agent Notifier {}", env!("CARGO_PKG_VERSION"));
    println!("data: {}", config::data_dir().display());
    println!("config: {}", config::config_path().display());
    match zbus::blocking::Connection::session().and_then(|c| {
        zbus::blocking::Proxy::new(
            &c,
            "io.github.mmmohebi.AgentNotifier",
            "/io/github/mmmohebi/AgentNotifier",
            "io.github.mmmohebi.AgentNotifier1",
        )
        .map(|_| ())
    }) {
        Ok(()) => println!("D-Bus: available"),
        Err(e) => println!("D-Bus: unavailable ({e})"),
    };
    match config::load(&config::config_path()) {
        Ok(c) => println!("destinations: {}", c.destinations.len()),
        Err(e) => println!("configuration error: {e:#}"),
    }
}
