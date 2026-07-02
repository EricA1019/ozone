.PHONY: install install-lite install-base build test lint preflight release-smoke release-gates prune-artifacts prune-artifacts-dry-run sync verify-install-parity setup-hooks graphify-refresh graphify-scope graphify-tui-core update-oz

# Build release binaries and sync into ~/.cargo/bin + ~/.local/bin (checksum-aware)
install: sync

sync:
	./contrib/sync-local-install.sh

verify-install-parity:
	./contrib/sync-local-install.sh --verify-only

# One-time dev setup: symlink contrib/hooks/* into .git/hooks/
setup-hooks:
	./contrib/install-dev-hooks.sh

graphify-refresh:
	./contrib/refresh-graphify.sh

graphify-scope:
	bash ./contrib/graphify-scope.sh $(if $(SCOPE),$(SCOPE),ozone-tui-core)

graphify-tui-core:
	bash ./contrib/graphify-scope.sh ozone-tui-core

install-lite:
	cargo install --path . --locked --profile release-lite

install-base:
	cargo install --path . --locked --features full --profile release

build:
	cargo build --release

test:
	cargo test --workspace

lint:
	cargo clippy --workspace --all-targets -- -D warnings

# Run lint + test before committing
preflight: lint test
	@echo "✅ Preflight passed — safe to commit"

release-gates: preflight verify-install-parity
	$(MAKE) release-smoke
	@echo "✅ Release gates passed — workspace green and installed binaries match release artifacts"

release-smoke:
	cargo build --release -p ozone -p ozone-mcp-app
	cargo test -p ozone-mcp release_smoke_gate -- --ignored --nocapture --test-threads=1

prune-artifacts:
	./contrib/prune-build-artifacts.sh

prune-artifacts-dry-run:
	./contrib/prune-build-artifacts.sh --dry-run

# Quick alias update: build release binary and copy to ~/.local/bin/oz
update-oz:
	cd "$(shell dirname $(realpath $(firstword $(MAKEFILE_LIST))))" && cargo build --release --bin ozone
	cp "$(shell dirname $(realpath $(firstword $(MAKEFILE_LIST))))/target/release/ozone" $(HOME)/.local/bin/oz
	cp "$(shell dirname $(realpath $(firstword $(MAKEFILE_LIST))))/target/release/ozone" $(HOME)/.local/bin/ozone
	@echo "✅ oz/ozone aliases updated"
