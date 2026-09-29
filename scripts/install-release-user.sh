#!/bin/sh
set -eu

release_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
data_home=${XDG_DATA_HOME:-"$HOME/.local/share"}
bin_dir="$HOME/.local/libexec"
command_dir="$HOME/.local/bin"
extension_dir="$data_home/gnome-shell/extensions/agent-notifier@mmmohebi.github.io"

mkdir -p "$bin_dir" "$command_dir" "$data_home/dbus-1/services" \
    "$HOME/.config/systemd/user" "$extension_dir"
install -m755 "$release_dir/agent-notifier" "$bin_dir/agent-notifier"
ln -sfn ../libexec/agent-notifier "$command_dir/agent-notifier"
sed "s|@LIBEXECDIR@|$bin_dir|g" "$release_dir/data/io.github.mmmohebi.AgentNotifier.service" \
    > "$data_home/dbus-1/services/io.github.mmmohebi.AgentNotifier.service"
sed "s|@LIBEXECDIR@|$bin_dir|g" "$release_dir/data/agent-notifier.service" \
    > "$HOME/.config/systemd/user/agent-notifier.service"
unzip -oq "$release_dir/agent-notifier@mmmohebi.github.io.zip" -d "$extension_dir"
glib-compile-schemas "$extension_dir/schemas"
systemctl --user daemon-reload
systemctl --user enable --now agent-notifier.service
"$bin_dir/agent-notifier" setup --apply --binary "$bin_dir/agent-notifier"

printf '%s\n' 'Installed Agent Notifier.'
printf '%s\n' 'Log out and back in, then enable agent-notifier@mmmohebi.github.io.'
