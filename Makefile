.PHONY: install build test

# Install both binaries locally after any code change
install:
	cargo install --path . --locked
	cargo install --path apps/ozone-plus --locked

build:
	cargo build --release

test:
	cargo test
