# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

WinTab is a Windows 10/11 desktop utility that adds browser-style tab grouping to any window. Written in Rust using raw Win32 API (`windows-sys` crate). Single-process architecture with no DLL injection — uses `WINEVENT_OUTOFCONTEXT` hooks to monitor window events.

## Build & Development Commands

All commands use `just` (task runner):

```
just build          # Debug build
just release        # Optimized release build (opt-level=z, fat LTO, stripped)
just run            # Build and run debug
just test           # Run unit + acceptance tests (cargo test)
just test-all       # Full suite: unit/acceptance first, then serial desktop-switch E2E
just test-e2e       # E2E acceptance tests with screenshots + HTML report
just lint           # Clippy with -D warnings
just fmt            # Format code
just fmt-check      # Check formatting
just check          # Type-check without building
```

Run a single test: `cargo test test_name`

## Architecture

**Single-threaded message loop** — all state lives in a thread-local `RefCell<AppState>` accessed via `with_state()` / `try_with_state()`. No `unsafe` needed for state management.

### Module Responsibilities

- **`main.rs`** — Entry point, window class registration, message loop, panic hook (shows hidden windows on crash)
- **`state.rs`** — `AppState` central struct, event dispatch (`on_window_created`, `on_focus_changed`, etc.)
- **`window.rs`** — Window discovery/filtering via `is_eligible()`, metadata extraction (title, icon, rect). Uses DWM extended frame bounds.
- **`hook.rs`** — `SetWinEventHook` for 9 event types (create, destroy, move, focus, minimize, etc.)
- **`group.rs`** — `TabGroup` (tabs vec + active index) and `GroupManager` (group↔window mappings). `switch_to()` does atomic show/hide.
- **`overlay.rs`** — GDI-based tab bar rendering via `UpdateLayeredWindow`. Hit testing, hover tracking, tooltips. Overlays are per-group, created lazily.
- **`preview.rs`** — DWM Thumbnail preview on tab hover. `PreviewManager` registers/unregisters DWM thumbnails, 500ms hover delay via `SetTimer`, aspect-ratio-correct sizing.
- **`drag.rs`** — Drag-and-drop state machine with 5px threshold. Drop targets: overlay (merge groups), managed window (create group), empty space (detach).
- **`config.rs`** — YAML config loading from `%APPDATA%\WinTab\config.yaml`. Auto-grouping rules engine (match by process_name, class_name, title; operators: equals, contains, starts_with, ends_with, regex).
- **`position_store.rs`** — Persists window group position/size to YAML. Restores on re-group.
- **`vdesktop.rs`** — Virtual desktop detection via raw COM vtable dispatch for `IVirtualDesktopManager`. `#[cfg(test)]` mock support.
- **`appdata.rs`** — `%APPDATA%\WinTab\` directory management.
- **`tray.rs`** — System tray icon with Enable/Disable toggle and Exit.
- **`acceptance.rs`** — E2E acceptance tests using `dummy_window.exe` subprocess, screenshot evidence.
- **`screenshot.rs`** — Test-only BitBlt+CAPTUREBLT screen capture, saves PNGs to `evidence/`.

### Key Patterns

- **RefCell re-entrancy safety**: `try_with_state()` exists because Win32 callbacks can re-enter during `with_state()` borrows. Never call Win32 APIs that pump messages while holding a `with_state()` borrow. When passing state to methods called from wndproc callbacks, pass disjoint struct references (e.g., `&GroupManager`, `&OverlayManager`) instead of re-entering the `RefCell`.
- **Overlay windows**: `WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST` — they float above grouped windows without stealing focus.
- **Tab rendering**: 32-bit ARGB bitmap with `premultiply_pixel()` for alpha blending. Tab height 28px, icon 16x16.

## Testing

133 tests (131 run + 2 ignored desktop-switch tests). Three tiers:

1. **Unit tests** — Pure logic, no Win32 calls. Inline `#[cfg(test)]` modules in `drag.rs`, `group.rs`, `overlay.rs`, `config.rs`, `position_store.rs`, `preview.rs`, `vdesktop.rs`. Mock HWNDs via `HWND(1)`, `HWND(2)` etc.
2. **Acceptance tests** — E2E tests in `acceptance.rs` using `dummy_window.exe` (real Win32 windows). Screenshots saved to `evidence/<test_name>/` (gitignored). Run with `just test` or `just test-e2e`.
3. **Desktop-switch E2E** — 2 `#[ignore]` tests that use `SendInput` to switch virtual desktops. Run with `just test-all` (serial, `--test-threads=1`).

**Convention:** New features that create or manipulate windows must have E2E acceptance tests with screenshot evidence. See `acceptance.rs` for patterns (spawn dummy windows, exercise feature, capture screenshots, assert state).

## Dependencies

- `windows-sys 0.59` — Win32 API bindings (feature gates in `Cargo.toml`)
- `serde` + `serde_yaml` — Config and position store serialization
- `regex` — Config rule matching
- `image` (dev-dependency) — PNG encoding for E2E screenshot evidence

## Platform

Windows only. Requires Windows 10+ with DWM compositing (always on since Win 8).
