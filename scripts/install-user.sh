#!/bin/sh
set -eu
binary=${1:-target/release/agent-notifier}
test -x "$binary" || { echo "Build first with: cargo build --release" >&2; exit 1; }
data_home=${XDG_DATA_HOME:-"$HOME/.local/share"}
bin_dir="$HOME/.local/libexec"
command_dir="$HOME/.local/bin"
extension_dir="$data_home/gnome-shell/extensions/agent-notifier@mmmohebi.github.io"
mkdir -p "$bin_dir" "$command_dir" "$data_home/dbus-1/services" "$HOME/.config/systemd/user" "$extension_dir"
install -m755 "$binary" "$bin_dir/agent-notifier"
ln -sfn ../libexec/agent-notifier "$command_dir/agent-notifier"
sed "s|@LIBEXECDIR@|$bin_dir|g" data/io.github.mmmohebi.AgentNotifier.service > "$data_home/dbus-1/services/io.github.mmmohebi.AgentNotifier.service"
sed "s|@LIBEXECDIR@|$bin_dir|g" data/agent-notifier.service > "$HOME/.config/systemd/user/agent-notifier.service"
cp -R extension/. "$extension_dir/"
glib-compile-schemas "$extension_dir/schemas"
systemctl --user daemon-reload
systemctl --user enable --now agent-notifier.service
"$bin_dir/agent-notifier" setup --apply --binary "$bin_dir/agent-notifier"
echo "Installed. Enable agent-notifier@mmmohebi.github.io using Extensions after logging in again."
