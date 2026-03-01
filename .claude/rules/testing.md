# Testing Rules

## E2E Evidence Required for New Features

Any feature that creates, manipulates, or interacts with windows MUST include an E2E acceptance test with screenshot evidence before being marked as done.

### What this means

1. **Write the E2E test as part of the feature implementation**, not as a follow-up
2. Tests go in `src/acceptance.rs` following existing patterns
3. Screenshots are captured via `screenshot::capture_region()` and saved to `evidence/<test_name>/`
4. Test must assert observable state (window visibility, position, pixel checks where practical)

### E2E test pattern

```rust
#[test]
fn acceptance_e2e_feature_name() {
    // 1. Build and spawn dummy_window.exe with N windows
    // 2. Set up state (create groups, switch tabs, etc.)
    // 3. Exercise the feature
    // 4. Assert state + capture screenshot evidence
    // 5. Clean up (kill child process, destroy windows)
}
```

### When E2E is not needed

- Pure config parsing logic (unit tests suffice)
- Internal refactors with no behavior change
- Bug fixes where the existing E2E test already covers the scenario

## Test Hygiene

- Run `just test` before committing — all 131+ tests must pass
- Run `just lint` — zero clippy warnings
- New unit tests for pure functions (math, parsing, state transitions)
- `#[cfg(test)]` accessors are fine for exposing internal state to tests
