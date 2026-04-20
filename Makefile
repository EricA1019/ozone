.PHONY: install install-lite install-base install-plus build test lint preflight prune-artifacts prune-artifacts-dry-run

# Install both binaries locally after any code change
install:
	cargo install --path . --locked
	cargo install --path apps/ozone-plus --locked

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

prune-artifacts:
	./contrib/prune-build-artifacts.sh

prune-artifacts-dry-run:
	./contrib/prune-build-artifacts.sh --dry-run
