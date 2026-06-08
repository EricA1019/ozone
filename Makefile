.PHONY: install install-lite install-base install-plus build test lint preflight release-smoke release-gates prune-artifacts prune-artifacts-dry-run sync verify-install-parity setup-hooks graphify-refresh graphify-scope graphify-tui-core

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

install-plus:
	cargo install --path apps/ozone-plus --locked

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
	cargo test -p ozone-mcp release_smoke_gate -- --ignored --nocapture --test-threads=1

prune-artifacts:
	./contrib/prune-build-artifacts.sh

prune-artifacts-dry-run:
	./contrib/prune-build-artifacts.sh --dry-run
