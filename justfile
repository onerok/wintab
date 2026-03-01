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

# run full test suite: unit/acceptance tests first, then serial desktop-switch E2E tests
test-all:
    cargo build --bin dummy_window
    cargo test
    cargo test -- --ignored --test-threads=1 --nocapture

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

# run all E2E acceptance tests with dummy process + generate HTML report
test-e2e:
    cargo build --bin dummy_window
    mkdir -p evidence
    cargo test acceptance -- --test-threads=1 --nocapture 2>&1 | tee evidence/test-results.txt || true
    bash scripts/e2e-report.sh

# clean build artifacts
clean:
    cargo clean
