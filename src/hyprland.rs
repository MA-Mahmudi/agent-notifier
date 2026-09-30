use crate::model::{Session, State};
use anyhow::Context;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;
use zbus::blocking::{connection, Connection, Proxy};

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Launcher {
    Wofi,
    Rofi,
}

fn connect() -> anyhow::Result<Connection> {
    connection::Builder::session()?
        .method_timeout(Duration::from_secs(3))
        .build()
        .context("connect to the session D-Bus")
}

fn proxy(connection: &Connection) -> anyhow::Result<Proxy<'_>> {
    Ok(Proxy::new(
        connection,
        "io.github.mmmohebi.AgentNotifier",
        "/io/github/mmmohebi/AgentNotifier",
        "io.github.mmmohebi.AgentNotifier1",
    )?)
}

fn sessions(proxy: &Proxy<'_>, history_hours: u32) -> anyhow::Result<Vec<Session>> {
    let since = Utc::now().timestamp() - i64::from(history_hours) * 3600;
    let response: String = proxy.call("ListSessions", &since)?;
    serde_json::from_str(&response).context("invalid session response from companion")
}

pub fn waybar(history_hours: u32, completed_minutes: u32) -> anyhow::Result<()> {
    let result = (|| {
        let connection = connect()?;
        let proxy = proxy(&connection)?;
        sessions(&proxy, history_hours)
    })();
    let output = match result {
        Ok(sessions) => render(&sessions, Utc::now(), completed_minutes),
        Err(error) => {
            eprintln!("agent-notifier: {error:#}");
            json!({
                "text": "Agents !",
                "class": "unavailable",
                "tooltip": "Companion service unavailable. Run agent-notifier doctor and check systemctl --user status agent-notifier.service.",
            })
        }
    };
    println!("{output}");
    Ok(())
}

fn markup(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&apos;")
        .replace('"', "&quot;")
}

fn short(text: &str, max: usize) -> String {
    let text: String = text
        .chars()
        .filter(|character| !character.is_control() || character.is_whitespace())
        .collect();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max {
        text
    } else {
        text.chars().take(max).collect::<String>() + "…"
    }
}

fn state_label(state: &State) -> &'static str {
    match state {
        State::Working => "Working",
        State::NeedsAttention => "Attention",
        State::Completed => "Done",
        State::Failed => "Failed",
        State::Ended => "Ended",
        State::Unknown => "Unknown",
    }
}

fn relative(updated: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let minutes = (now - updated).num_minutes().max(0);
    if minutes == 0 {
        "just now".into()
    } else if minutes < 60 {
        format!("{minutes}m ago")
    } else {
        format!("{}h ago", minutes / 60)
    }
}

fn render(sessions: &[Session], now: DateTime<Utc>, completed_minutes: u32) -> Value {
    let visible: Vec<_> = sessions.iter().filter(|session| !session.hidden).collect();
    let active: Vec<_> = visible
        .iter()
        .copied()
        .filter(|session| match session.state {
            State::Working | State::NeedsAttention => true,
            State::Completed | State::Failed => {
                (now - session.updated_at).num_seconds() <= i64::from(completed_minutes) * 60
            }
            _ => false,
        })
        .collect();
    let class = if active.iter().any(|s| s.state == State::NeedsAttention) {
        "needs-attention"
    } else if active.iter().any(|s| s.state == State::Failed) {
        "failed"
    } else if active.iter().any(|s| s.state == State::Working) {
        "working"
    } else if !active.is_empty() {
        "completed"
    } else {
        "idle"
    };
    let mut chips: Vec<_> = active
        .iter()
        .take(4)
        .map(|session| {
            let color = match session.state {
                State::Working => "#62a0ea",
                State::Completed => "#57e389",
                _ => "#ff7b63",
            };
            let project = if session.project.is_empty() {
                &session.title
            } else {
                &session.project
            };
            format!(
                "<span foreground=\"{color}\">●</span> {} · {}",
                session.agent.as_str(),
                markup(&short(project, 12))
            )
        })
        .collect();
    if active.len() > 4 {
        chips.push(format!("+{}", active.len() - 4));
    }
    let text = if chips.is_empty() {
        format!("Agents {}", visible.len())
    } else {
        chips.join("  ")
    };
    let mut tooltip = vec![format!(
        "Agent Notifier · {} recent sessions",
        visible.len()
    )];
    for session in visible.iter().take(20) {
        tooltip.push(markup(&format!(
            "\n{} · {} · {} · {}\n{}\n{}",
            session.agent.as_str(),
            short(&session.project, 40),
            state_label(&session.state),
            relative(session.updated_at, now),
            short(&session.title, 100),
            short(&session.preview, 260),
        )));
    }
    if visible.len() > 20 {
        tooltip.push(format!(
            "\n+{} more sessions in the menu",
            visible.len() - 20
        ));
    }
    tooltip.push("\nClick to manage sessions".into());
    json!({"text": text, "tooltip": tooltip.join("\n"), "class": class, "alt": class})
}

fn installed(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|dir| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(dir.join(program))
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
    })
}

fn choose(launcher: Launcher, prompt: &str, entries: &[String]) -> anyhow::Result<Option<usize>> {
    let mut command = match launcher {
        Launcher::Wofi => {
            let mut command = Command::new("wofi");
            command.args([
                "--dmenu",
                "--insensitive",
                "--no-custom-entry",
                "--prompt",
                prompt,
                "--cache-file",
                "/dev/null",
                "--define",
                "allow_markup=false",
                "--define",
                "allow_images=false",
                "--define",
                "print_line_num=false",
            ]);
            command
        }
        Launcher::Rofi => {
            let mut command = Command::new("rofi");
            command.args([
                "-dmenu",
                "-i",
                "-no-custom",
                "-no-markup-rows",
                "-format",
                "s",
                "-p",
                prompt,
            ]);
            command
        }
    };
    // Numbered rows disambiguate sessions with identical project/title text.
    let rows: Vec<_> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| format!("{}. {}", index + 1, short(entry, 600)))
        .collect();
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("open session menu; install wofi or a Wayland-compatible rofi")?;
    let write_result = child
        .stdin
        .take()
        .unwrap()
        .write_all(rows.join("\n").as_bytes());
    let output = child.wait_with_output()?;
    if output.status.code() == Some(1) {
        return Ok(None); // Escape/cancel is a normal exit for both launchers.
    }
    anyhow::ensure!(
        output.status.success(),
        "menu launcher exited with {}",
        output.status
    );
    write_result?;
    let selection = String::from_utf8(output.stdout)?;
    let selection = selection.trim_end_matches(['\r', '\n']);
    let index = rows
        .iter()
        .position(|row| row == selection)
        .context("menu launcher returned an unknown selection")?;
    Ok(Some(index))
}

fn check_response(response: &str) -> anyhow::Result<()> {
    let value: Value = serde_json::from_str(response)?;
    anyhow::ensure!(
        value["ok"] == true,
        "{}",
        value["error"]
            .as_str()
            .unwrap_or("companion rejected action")
    );
    Ok(())
}

fn resume_command(session: &Session) -> String {
    let prefix = format!("{}:", session.agent.as_str());
    let id = session.id.strip_prefix(&prefix).unwrap_or(&session.id);
    let quoted = format!("'{}'", id.replace('\'', "'\\''"));
    match session.agent {
        crate::model::Agent::Codex => format!("codex resume {quoted}"),
        crate::model::Agent::Claude => format!("claude --resume {quoted}"),
    }
}

fn copy_resume(session: &Session) -> anyhow::Result<()> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .spawn()
        .context("install wl-clipboard to copy resume commands")?;
    let write_result = child
        .stdin
        .take()
        .unwrap()
        .write_all(resume_command(session).as_bytes());
    let status = child.wait()?;
    write_result?;
    anyhow::ensure!(status.success(), "wl-copy exited with {status}");
    Ok(())
}

pub fn menu(launcher: Option<Launcher>, history_hours: u32) -> anyhow::Result<()> {
    let launcher = launcher
        .or_else(|| {
            if installed("wofi") {
                Some(Launcher::Wofi)
            } else if installed("rofi") {
                Some(Launcher::Rofi)
            } else {
                None
            }
        })
        .context("install wofi or a Wayland-compatible rofi to open the session menu")?;
    let connection = connect()?;
    let proxy = proxy(&connection)?;
    let mut hidden = false;
    loop {
        let sessions = sessions(&proxy, history_hours)?;
        let visible: Vec<_> = sessions.iter().filter(|s| s.hidden == hidden).collect();
        let hidden_count = sessions.iter().filter(|s| s.hidden).count();
        let mut entries = vec![
            "Enable all notifications".into(),
            "Disable all notifications".into(),
            if hidden {
                "Back to visible sessions".into()
            } else {
                format!("Hidden sessions ({hidden_count})")
            },
        ];
        entries.extend(visible.iter().map(|session| {
            format!(
                "{} · {} · {} · {} · {}",
                state_label(&session.state),
                session.agent.as_str(),
                session.project,
                short(&session.title, 100),
                relative(session.updated_at, Utc::now()),
            )
        }));
        let prompt = if hidden {
            "Hidden agent sessions"
        } else {
            "Agent Notifier"
        };
        let Some(selected) = choose(launcher, prompt, &entries)? else {
            return Ok(());
        };
        match selected {
            0 | 1 => {
                let response: String =
                    proxy.call("SetAllSessionNotifications", &(selected == 0))?;
                check_response(&response)?;
            }
            2 => hidden = !hidden,
            _ => {
                let session = visible[selected - 3];
                let actions = vec![
                    "Copy resume command".into(),
                    if session.notifications_enabled {
                        "Notify off".into()
                    } else {
                        "Notify on".into()
                    },
                    if session.hidden {
                        "Restore session".into()
                    } else {
                        "Hide session".into()
                    },
                    "Back".into(),
                    format!(
                        "Preview: {}",
                        if session.preview.is_empty() {
                            "No preview available"
                        } else {
                            &session.preview
                        }
                    ),
                ];
                if let Some(action) = choose(launcher, &short(&session.title, 80), &actions)? {
                    match action {
                        0 => copy_resume(session)?,
                        1 | 2 => {
                            let (method, enabled) = if action == 1 {
                                ("SetSessionNotifications", !session.notifications_enabled)
                            } else {
                                ("SetSessionHidden", !session.hidden)
                            };
                            let response: String = proxy.call(method, &(&session.id, enabled))?;
                            check_response(&response)?;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Agent;
    use chrono::TimeDelta;

    fn session(state: State) -> Session {
        let now = Utc::now();
        Session {
            id: "codex:example".into(),
            agent: Agent::Codex,
            title: "My session".into(),
            project: "project".into(),
            state,
            preview: "preview".into(),
            created_at: now,
            updated_at: now,
            live: true,
            notifications_enabled: false,
            hidden: false,
        }
    }

    #[test]
    fn hidden_and_expired_sessions_do_not_make_active_chips() {
        let mut hidden = session(State::NeedsAttention);
        hidden.hidden = true;
        let mut expired = session(State::Completed);
        expired.updated_at -= TimeDelta::minutes(11);
        let output = render(&[hidden, expired], Utc::now(), 10);
        assert_eq!(output["class"], "idle");
        assert_eq!(output["text"], "Agents 1");
        assert!(output["tooltip"].as_str().unwrap().contains("Done"));
        assert!(!output["tooltip"].as_str().unwrap().contains("Attention"));
    }

    #[test]
    fn attention_and_failure_take_priority_over_working_and_completion() {
        let now = Utc::now();
        let mut sessions = vec![session(State::Completed), session(State::Working)];
        assert_eq!(render(&sessions, now, 10)["class"], "working");
        sessions.push(session(State::Failed));
        assert_eq!(render(&sessions, now, 10)["class"], "failed");
        sessions.push(session(State::NeedsAttention));
        assert_eq!(render(&sessions, now, 10)["class"], "needs-attention");
    }

    #[test]
    fn output_escapes_untrusted_markup_and_limits_chips() {
        let mut item = session(State::Working);
        item.project = "<b>&测试\0\u{1f}".into();
        item.preview = "<span>secret-looking & text</span>".into();
        let output = render(&vec![item; 6], Utc::now(), 10);
        let text = output["text"].as_str().unwrap();
        assert!(text.contains("&lt;b&gt;&amp;测试"));
        assert!(!text.chars().any(char::is_control));
        assert_eq!(text.matches("foreground=").count(), 4);
        assert!(text.ends_with("+2"));
        assert!(output["tooltip"].as_str().unwrap().contains("&lt;span&gt;"));
        assert!(!output.to_string().contains('\n'));
    }

    #[test]
    fn resume_commands_quote_shell_metacharacters_without_executing_them() {
        let mut item = session(State::Working);
        item.id = "codex:a'$(touch /tmp/unsafe)".into();
        assert_eq!(
            resume_command(&item),
            "codex resume 'a'\\''$(touch /tmp/unsafe)'"
        );
        item.agent = Agent::Claude;
        item.id = "claude:example".into();
        assert_eq!(resume_command(&item), "claude --resume 'example'");
    }
}
