#!/bin/sh
set -eu
mkdir -p dist
glib-compile-schemas --strict --dry-run extension/schemas
package_dir=$(mktemp -d)
trap 'rm -rf "$package_dir"' EXIT INT TERM
mkdir -p "$package_dir/schemas"
install -m644 extension/extension.js extension/prefs.js extension/metadata.json extension/stylesheet.css LICENSE "$package_dir/"
install -m644 extension/schemas/org.gnome.shell.extensions.agent-notifier.gschema.xml "$package_dir/schemas/"
(cd "$package_dir" && zip -qr extension.zip extension.js prefs.js metadata.json stylesheet.css LICENSE schemas)
install -m644 "$package_dir/extension.zip" dist/agent-notifier@mmmohebi.github.io.zip
echo "dist/agent-notifier@mmmohebi.github.io.zip"
