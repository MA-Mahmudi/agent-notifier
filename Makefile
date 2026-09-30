.PHONY: test check test-hyprland release extension preview packages
test:
	cargo test
check:
	python3 scripts/release-notes.py > /dev/null
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	glib-compile-schemas --strict --dry-run extension/schemas
	sh -n scripts/install-user.sh scripts/install-release-user.sh scripts/build-release-packages.sh
test-hyprland:
	cargo build --locked
	dbus-run-session -- python3 tests/hyprland.py "$(or $(CARGO_TARGET_DIR),target)/debug/agent-notifier"
release:
	cargo build --release --locked
extension:
	./scripts/package-extension.sh
preview:
	./scripts/create-preview-gif.py assets/agent-notifier-preview.gif
packages: release
	./scripts/build-release-packages.sh $${VERSION:?Set VERSION, for example VERSION=0.1.0}
