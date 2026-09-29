#!/bin/sh
set -eu
mkdir -p dist
glib-compile-schemas extension/schemas
(cd extension && zip -qr ../dist/agent-notifier@mmmohebi.github.io.zip .)
echo "dist/agent-notifier@mmmohebi.github.io.zip"
