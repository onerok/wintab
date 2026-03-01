# wintab - window tabbing for Windows

# default: list recipes
default:
    @just --list

# build debug
build:
    cargo build

# build release (optimized, stripped)
release:
    cargo build --release

# run debug build
run:
    cargo run

# run release build
run-release:
    cargo run --release

# run tests
test:
    cargo test

# check for errors without building
check:
    cargo check

# run clippy lints
lint:
    cargo clippy -- -D warnings

# format code
fmt:
    cargo fmt

# check formatting without modifying
fmt-check:
    cargo fmt -- --check

# install release binary to ~/.cargo/bin
install:
    cargo install --path .

# uninstall binary from ~/.cargo/bin
uninstall:
    cargo uninstall wintab

# run E2E acceptance test with dummy process
test-e2e:
    cargo build --bin dummy_window
    cargo test acceptance_rules_e2e -- --test-threads=1 --nocapture

# clean build artifacts
clean:
    cargo clean
