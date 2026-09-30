#!/bin/sh
set -eu

release_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
desktop=${1:-auto}
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
    *) echo "Usage: $0 [auto|gnome|hyprland]" >&2; exit 1 ;;
esac
data_home=${XDG_DATA_HOME:-"$HOME/.local/share"}
config_home=${XDG_CONFIG_HOME:-"$HOME/.config"}
bin_dir="$HOME/.local/libexec"
command_dir="$HOME/.local/bin"
extension_dir="$data_home/gnome-shell/extensions/agent-notifier@mmmohebi.github.io"
integration_dir="$data_home/agent-notifier/hyprland"

mkdir -p "$bin_dir" "$command_dir" "$data_home/dbus-1/services" \
    "$config_home/systemd/user" "$integration_dir"
install -m755 "$release_dir/agent-notifier" "$bin_dir/agent-notifier"
ln -sfn ../libexec/agent-notifier "$command_dir/agent-notifier"
sed "s|@LIBEXECDIR@|$bin_dir|g" "$release_dir/data/io.github.mmmohebi.AgentNotifier.service" \
    > "$data_home/dbus-1/services/io.github.mmmohebi.AgentNotifier.service"
sed "s|@LIBEXECDIR@|$bin_dir|g" "$release_dir/data/agent-notifier.service" \
    > "$config_home/systemd/user/agent-notifier.service"
install -m644 "$release_dir/hyprland/waybar.jsonc" "$release_dir/hyprland/style.css" "$integration_dir/"
if test "$desktop" = gnome; then
    mkdir -p "$extension_dir"
    unzip -oq "$release_dir/agent-notifier@mmmohebi.github.io.zip" -d "$extension_dir"
    glib-compile-schemas "$extension_dir/schemas"
fi
systemctl --user daemon-reload
systemctl --user enable --now agent-notifier.service
"$bin_dir/agent-notifier" setup --apply --binary "$bin_dir/agent-notifier"

printf '%s\n' 'Installed Agent Notifier.'
if test "$desktop" = hyprland; then
    printf '%s\n' "Add custom/agent-notifier to Waybar using $integration_dir/waybar.jsonc and style.css. See the README's Hyprland section."
else
    printf '%s\n' 'Log out and back in, then enable agent-notifier@mmmohebi.github.io.'
fi
