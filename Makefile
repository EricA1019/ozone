.PHONY: install install-lite install-base install-plus build test

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
	cargo test
