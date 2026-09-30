#!/bin/sh
set -eu

version=${1:-}
binary=${2:-target/release/agent-notifier}
case "$version" in
    ''|*[!0-9A-Za-z.-]*)
        echo "Usage: $0 VERSION [BINARY]" >&2
        exit 1
        ;;
esac
test -x "$binary" || {
    echo "Release binary not found: $binary" >&2
    exit 1
}

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
binary=$(CDPATH= cd -- "$(dirname -- "$binary")" && pwd)/$(basename -- "$binary")
cd "$project_dir"
work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT INT TERM
mkdir -p "$project_dir/dist"

"$project_dir/scripts/package-extension.sh"
install -m644 "$project_dir/dist/agent-notifier@mmmohebi.github.io.zip" \
    "$project_dir/dist/agent-notifier-shell-extension-$version.zip"

archive_dir="$work_dir/agent-notifier-$version-linux-x86_64"
mkdir -p "$archive_dir/data" "$archive_dir/hyprland"
install -m644 "$project_dir/hyprland/waybar.jsonc" "$project_dir/hyprland/style.css" "$archive_dir/hyprland/"
install -m755 "$binary" "$archive_dir/agent-notifier"
install -m755 "$project_dir/scripts/install-release-user.sh" "$archive_dir/install.sh"
install -m644 "$project_dir/dist/agent-notifier@mmmohebi.github.io.zip" "$archive_dir/"
install -m644 "$project_dir/data/agent-notifier.service" "$archive_dir/data/"
install -m644 "$project_dir/data/io.github.mmmohebi.AgentNotifier.service" "$archive_dir/data/"
install -m644 "$project_dir/README.md" "$project_dir/CHANGELOG.md" "$project_dir/LICENSE" "$archive_dir/"
tar -C "$work_dir" -czf "$project_dir/dist/agent-notifier-$version-linux-x86_64.tar.gz" \
    "$(basename -- "$archive_dir")"

deb_root="$work_dir/deb"
mkdir -p "$deb_root/DEBIAN" "$deb_root/usr/libexec" "$deb_root/usr/bin" \
    "$deb_root/usr/lib/systemd/user" "$deb_root/usr/share/dbus-1/services" \
    "$deb_root/usr/share/doc/agent-notifier" "$deb_root/usr/share/agent-notifier/hyprland"
installed_size=$(du -k "$binary" | cut -f1)
cat > "$deb_root/DEBIAN/control" <<EOF
Package: agent-notifier
Version: $version
Section: utils
Priority: optional
Architecture: amd64
Installed-Size: $installed_size
Maintainer: Mohammad Mohebi <mmmohebi@users.noreply.github.com>
Depends: systemd, dbus-user-session
Description: Local Codex and Claude Code session monitor
 Rust companion service with GNOME Shell and Hyprland/Waybar integrations.
EOF
install -m755 "$binary" "$deb_root/usr/libexec/agent-notifier"
install -m644 "$project_dir/hyprland/waybar.jsonc" "$project_dir/hyprland/style.css" "$deb_root/usr/share/agent-notifier/hyprland/"
ln -s ../libexec/agent-notifier "$deb_root/usr/bin/agent-notifier"
sed 's|@LIBEXECDIR@|/usr/libexec|g' "$project_dir/data/agent-notifier.service" \
    > "$deb_root/usr/lib/systemd/user/agent-notifier.service"
sed 's|@LIBEXECDIR@|/usr/libexec|g' "$project_dir/data/io.github.mmmohebi.AgentNotifier.service" \
    > "$deb_root/usr/share/dbus-1/services/io.github.mmmohebi.AgentNotifier.service"
install -m644 "$project_dir/README.md" "$deb_root/usr/share/doc/agent-notifier/README.md"
install -m644 "$project_dir/CHANGELOG.md" "$deb_root/usr/share/doc/agent-notifier/CHANGELOG.md"
install -m644 "$project_dir/LICENSE" "$deb_root/usr/share/doc/agent-notifier/copyright"
dpkg-deb --build --root-owner-group "$deb_root" \
    "$project_dir/dist/agent-notifier_${version}_amd64.deb"

if command -v rpmbuild >/dev/null 2>&1; then
    rpm_top="$work_dir/rpmbuild"
    rpm_source="$work_dir/agent-notifier-$version"
    mkdir -p "$rpm_top/BUILD" "$rpm_top/BUILDROOT" "$rpm_top/RPMS" \
        "$rpm_top/SOURCES" "$rpm_top/SPECS" "$rpm_top/SRPMS" "$rpm_source"
    install -m755 "$binary" "$rpm_source/agent-notifier"
    mkdir -p "$rpm_source/hyprland"
    install -m644 "$project_dir/hyprland/waybar.jsonc" "$project_dir/hyprland/style.css" "$rpm_source/hyprland/"
    sed 's|@LIBEXECDIR@|/usr/libexec|g' "$project_dir/data/agent-notifier.service" \
        > "$rpm_source/agent-notifier.service"
    sed 's|@LIBEXECDIR@|/usr/libexec|g' "$project_dir/data/io.github.mmmohebi.AgentNotifier.service" \
        > "$rpm_source/io.github.mmmohebi.AgentNotifier.service"
    install -m644 "$project_dir/README.md" "$project_dir/CHANGELOG.md" "$project_dir/LICENSE" "$rpm_source/"
    tar -C "$work_dir" -czf "$rpm_top/SOURCES/agent-notifier-$version.tar.gz" \
        "$(basename -- "$rpm_source")"
    install -m644 "$project_dir/packaging/agent-notifier.spec" "$rpm_top/SPECS/"
    rpmbuild -bb --define "_topdir $rpm_top" --define "package_version $version" \
        "$rpm_top/SPECS/agent-notifier.spec"
    find "$rpm_top/RPMS" -type f -name '*.rpm' -exec cp {} "$project_dir/dist/" \;
fi

printf '%s\n' "Release packages written to $project_dir/dist"
