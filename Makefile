.PHONY: test check release extension preview packages
test:
	cargo test
check:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	glib-compile-schemas --strict --dry-run extension/schemas
release:
	cargo build --release --locked
extension:
	./scripts/package-extension.sh
preview:
	./scripts/create-preview-gif.py assets/agent-notifier-preview.gif
packages: release
	./scripts/build-release-packages.sh $${VERSION:?Set VERSION, for example VERSION=0.1.0}
