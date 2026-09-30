#!/bin/sh
set -eu
binary=${1:-target/release/agent-notifier}
desktop=${2:-auto}
case "$desktop" in
    auto)
        case "${XDG_CURRENT_DESKTOP:-}" in
            *[Hh][Yy][Pp][Rr][Ll][Aa][Nn][Dd]*) desktop=hyprland ;;
            *)
                if test -n "${HYPRLAND_INSTANCE_SIGNATURE:-}"; then desktop=hyprland; else desktop=gnome; fi
                ;;
        esac
        ;;
    gnome|hyprland) ;;
    *) echo "Usage: $0 [BINARY] [auto|gnome|hyprland]" >&2; exit 1 ;;
esac
test -x "$binary" || { echo "Build first with: cargo build --release" >&2; exit 1; }
data_home=${XDG_DATA_HOME:-"$HOME/.local/share"}
config_home=${XDG_CONFIG_HOME:-"$HOME/.config"}
bin_dir="$HOME/.local/libexec"
command_dir="$HOME/.local/bin"
extension_dir="$data_home/gnome-shell/extensions/agent-notifier@mmmohebi.github.io"
integration_dir="$data_home/agent-notifier/hyprland"
mkdir -p "$bin_dir" "$command_dir" "$data_home/dbus-1/services" "$config_home/systemd/user" "$integration_dir"
install -m755 "$binary" "$bin_dir/agent-notifier"
ln -sfn ../libexec/agent-notifier "$command_dir/agent-notifier"
sed "s|@LIBEXECDIR@|$bin_dir|g" data/io.github.mmmohebi.AgentNotifier.service > "$data_home/dbus-1/services/io.github.mmmohebi.AgentNotifier.service"
sed "s|@LIBEXECDIR@|$bin_dir|g" data/agent-notifier.service > "$config_home/systemd/user/agent-notifier.service"
install -m644 hyprland/waybar.jsonc hyprland/style.css "$integration_dir/"
if test "$desktop" = gnome; then
    mkdir -p "$extension_dir"
    cp -R extension/. "$extension_dir/"
    glib-compile-schemas "$extension_dir/schemas"
fi
systemctl --user daemon-reload
systemctl --user enable --now agent-notifier.service
"$bin_dir/agent-notifier" setup --apply --binary "$bin_dir/agent-notifier"
if test "$desktop" = hyprland; then
    echo "Installed. Add custom/agent-notifier to Waybar using $integration_dir/waybar.jsonc and style.css. See the README's Hyprland section."
else
    echo "Installed. Enable agent-notifier@mmmohebi.github.io using Extensions after logging in again."
fi
