# Changelog

Every release must have a dated entry here. The release workflow publishes the matching entry as its GitHub release notes and rejects missing entries or version mismatches.

## [0.1.4] - 2026-09-30

### Added

- Hyprland support through a Waybar module with colored session chips, recent activity tooltips, and working-state animation.
- A wofi/rofi session menu with safe resume-command copying, per-session and global notification controls, and hide/restore actions.
- `agent-notifier waybar` and `agent-notifier menu` commands with configurable history windows and completion display duration.
- Private D-Bus integration tests covering state updates, menu actions, clipboard behavior, and Hyprland installation.

### Changed

- User installers detect Hyprland and support an explicit desktop choice, allowing installation without GNOME Shell or GSettings compilation.
- Release archives, Debian packages, and RPM packages include the Waybar configuration and stylesheet.
- GNOME and Hyprland share the existing companion, session history, and persisted notification choices.
- Release notes now come from this changelog, and publishing requires a matching version entry.

### Validation

- Rust tests, Clippy, formatter checks, integration tests, and GTK stylesheet parsing pass.
- Visual testing in a live Hyprland/Waybar session is still pending.

## [0.1.3] - 2026-09-29

### Fixed

- Clear attention status after tool execution so sessions return to working during the same turn.

## [0.1.2] - 2026-09-29

### Added

- Safe Codex and Claude Code resume-command copying from session cards.

### Fixed

- Require a user prompt before displaying a session and remove promptless imported sessions.

## [0.1.1] - 2026-09-29

### Fixed

- Restore hidden sessions when they are resumed while preserving hiding during the same run.

## [0.1.0] - 2026-09-29

### Added

- Initial GNOME Shell extension and Rust companion for Codex and Claude Code session monitoring.
- Hook setup, SQLite session history, persisted notification controls, ntfy/webhook delivery, and Secret Service credential storage.
- User-local archives, Debian and RPM packages, checksums, and release attestations.
