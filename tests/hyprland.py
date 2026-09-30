"""Run with: dbus-run-session -- python3 tests/hyprland.py BINARY."""

import ast
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import unittest


def mock_launcher():
    rows = sys.stdin.read().splitlines()
    queue_path = Path(os.environ["MENU_QUEUE"])
    queue = json.loads(queue_path.read_text())
    choice = queue.pop(0)
    queue_path.write_text(json.dumps(queue))
    with open(os.environ["MENU_LOG"], "a") as log:
        log.write(json.dumps({"rows": rows, "args": sys.argv[2:]}) + "\n")
    if choice is None:
        sys.exit(1)
    matches = [row for row in rows if choice in row]
    if len(matches) != 1:
        raise RuntimeError(f"Expected one match for {choice!r}: {rows!r}")
    print(matches[0])


if len(sys.argv) > 1 and sys.argv[1] == "--mock-launcher":
    mock_launcher()
    sys.exit(0)
if len(sys.argv) > 1 and sys.argv[1] == "--mock-clipboard":
    Path(os.environ["CLIPBOARD_FILE"]).write_text(sys.stdin.read())
    sys.exit(0)

BINARY = Path(sys.argv.pop(1)).resolve()
PROJECT = Path(__file__).resolve().parent.parent


class HyprlandIntegration(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="agent-notifier-hyprland-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = dict(os.environ)
        self.env.update(
            HOME=str(self.root),
            XDG_DATA_HOME=str(self.root / "data"),
            XDG_CONFIG_HOME=str(self.root / "config"),
            MENU_QUEUE=str(self.root / "queue.json"),
            MENU_LOG=str(self.root / "menu.jsonl"),
            CLIPBOARD_FILE=str(self.root / "clipboard"),
        )
        mock_bin = self.root / "bin"
        mock_bin.mkdir()
        self.env["PATH"] = str(mock_bin) + os.pathsep + self.env["PATH"]
        runner = f"exec {shlex.quote(sys.executable)} {shlex.quote(str(Path(__file__).resolve()))}"
        for name in ["wofi", "rofi"]:
            self.executable(mock_bin / name, f'{runner} --mock-launcher "$@"')
        self.executable(mock_bin / "wl-copy", f'{runner} --mock-clipboard "$@"')
        self.daemon = subprocess.Popen(
            [str(BINARY), "daemon"], env=self.env,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        self.addCleanup(self.stop_daemon)
        deadline = time.monotonic() + 5
        while True:
            # Calling our service before it owns its name can activate an installed
            # companion with the bus daemon's HOME. Query the bus without activation.
            owner = subprocess.run([
                "gdbus", "call", "--session", "--dest", "org.freedesktop.DBus",
                "--object-path", "/org/freedesktop/DBus",
                "--method", "org.freedesktop.DBus.NameHasOwner",
                "io.github.mmmohebi.AgentNotifier",
            ], env=self.env, capture_output=True, text=True, check=True, timeout=5)
            if "true" in owner.stdout:
                self.assertIsNone(self.daemon.poll(), "isolated companion exited")
                break
            if time.monotonic() >= deadline or self.daemon.poll() is not None:
                self.fail("companion did not start on the private session bus")
            time.sleep(0.05)
        self.assertEqual(self.call("GetServiceStatus")["config"],
                         str(self.root / "config/agent-notifier/config.toml"))
        self.call("IngestEvent", "codex", json.dumps({
            "session_id": "hyprland-session",
            "hook_event_name": "UserPromptSubmit",
            "cwd": "/tmp/<project>&test",
            "prompt": "Hyprland test title",
        }))

    @staticmethod
    def executable(path, body):
        path.write_text("#!/bin/sh\n" + body + "\n")
        path.chmod(0o755)

    def stop_daemon(self):
        if self.daemon.poll() is None:
            self.daemon.terminate()
            self.daemon.wait(timeout=5)

    def call(self, method, *args):
        result = subprocess.run([
            "gdbus", "call", "--session", "--dest", "io.github.mmmohebi.AgentNotifier",
            "--object-path", "/io/github/mmmohebi/AgentNotifier",
            "--method", f"io.github.mmmohebi.AgentNotifier1.{method}", *args,
        ], env=self.env, capture_output=True, text=True, check=True, timeout=5)
        return json.loads(ast.literal_eval(result.stdout)[0])

    def run_binary(self, *args, env=None):
        return subprocess.run([str(BINARY), *args], env=env or self.env,
                              capture_output=True, text=True, check=True, timeout=5)

    def run_menu(self, choices, launcher="wofi"):
        Path(self.env["MENU_QUEUE"]).write_text(json.dumps(choices))
        self.run_binary("menu", "--launcher", launcher)
        self.assertEqual(json.loads(Path(self.env["MENU_QUEUE"]).read_text()), [])

    def session(self):
        return self.call("ListSessions", "0")[0]

    def test_live_states_and_unavailable_output(self):
        working = json.loads(self.run_binary("waybar").stdout)
        self.assertEqual(working["class"], "working")
        self.assertIn("&lt;project&gt;&amp;", working["text"])
        for event, state in [("PermissionRequest", "needs-attention"), ("Stop", "completed")]:
            self.call("IngestEvent", "codex", json.dumps({
                "session_id": "hyprland-session", "hook_event_name": event,
                "message": "Preview <b> & text",
            }))
            status = json.loads(self.run_binary("waybar").stdout)
            self.assertEqual(status["class"], state)
            self.assertIn("Preview &lt;b&gt; &amp; text", status["tooltip"])
        unavailable_env = dict(self.env, DBUS_SESSION_BUS_ADDRESS="unix:path=/nonexistent-agent-notifier-bus")
        status = json.loads(self.run_binary("waybar", env=unavailable_env).stdout)
        self.assertEqual(status["class"], "unavailable")

    def test_menu_copy_toggle_hide_restore_and_global_default(self):
        self.run_menu(["Hyprland test title", "Copy resume command", None])
        self.assertEqual(Path(self.env["CLIPBOARD_FILE"]).read_text(), "codex resume 'hyprland-session'")
        self.run_menu(["Hyprland test title", "Notify on", None])
        self.assertTrue(self.session()["notifications_enabled"])
        self.run_menu(["Hyprland test title", "Hide session", None])
        self.assertTrue(self.session()["hidden"])
        self.assertEqual(json.loads(self.run_binary("waybar").stdout)["text"], "Agents 0")
        self.run_menu(["Hidden sessions (1)", "Hyprland test title", "Restore session", None])
        self.assertFalse(self.session()["hidden"])
        self.run_menu(["Enable all notifications", None], launcher="rofi")
        self.assertTrue(self.call("GetServiceStatus")["notifications_enabled_by_default"])
        self.run_menu(["Disable all notifications", None], launcher="rofi")
        self.assertFalse(self.session()["notifications_enabled"])
        self.assertFalse(self.call("GetServiceStatus")["notifications_enabled_by_default"])

    def test_cancel_and_preview_are_safe(self):
        self.run_menu(["Hyprland test title", "Preview:", None])
        self.run_menu([None])
        logs = [json.loads(row) for row in Path(self.env["MENU_LOG"]).read_text().splitlines()]
        self.assertIn("allow_markup=false", logs[0]["args"])
        self.assertIn("allow_images=false", logs[0]["args"])
        self.assertFalse(self.session()["notifications_enabled"])
        self.assertFalse(Path(self.env["CLIPBOARD_FILE"]).exists())

    def test_source_and_release_installers_without_gnome(self):
        self.stop_daemon()
        mock_bin = self.root / "bin"
        self.executable(mock_bin / "systemctl", "exit 0")
        self.executable(mock_bin / "glib-compile-schemas", "exit 99")
        for kind in ["source", "release"]:
            install_home = self.root / kind
            install_home.mkdir()
            env = dict(self.env, HOME=str(install_home),
                       XDG_DATA_HOME=str(install_home / "data"),
                       XDG_CONFIG_HOME=str(install_home / "config"),
                       XDG_CURRENT_DESKTOP="Hyprland")
            if kind == "source":
                command = [str(PROJECT / "scripts/install-user.sh"), str(BINARY)]
            else:
                archive = self.root / "release-archive"
                archive.mkdir()
                shutil.copy(BINARY, archive / "agent-notifier")
                shutil.copytree(PROJECT / "hyprland", archive / "hyprland")
                shutil.copytree(PROJECT / "data", archive / "data")
                shutil.copy(PROJECT / "scripts/install-release-user.sh", archive / "install.sh")
                command = [str(archive / "install.sh"), "hyprland"]
            subprocess.run(command, cwd=PROJECT, env=env, capture_output=True,
                           text=True, check=True, timeout=5)
            self.assertTrue((install_home / "data/agent-notifier/hyprland/waybar.jsonc").is_file())
            self.assertTrue((install_home / "config/systemd/user/agent-notifier.service").is_file())
            self.assertTrue((install_home / ".codex/hooks.json").is_file())
            self.assertTrue((install_home / ".claude/settings.json").is_file())
            opencode_plugin = install_home / "config/opencode/plugins/agent-notifier.js"
            self.assertTrue(opencode_plugin.is_file())
            self.assertNotIn("__AGENT_NOTIFIER_BINARY__", opencode_plugin.read_text())
            self.assertFalse((install_home / "data/gnome-shell").exists())


if __name__ == "__main__":
    unittest.main()
