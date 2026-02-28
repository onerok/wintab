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
just test           # Run unit tests (cargo test)
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
- **`overlay.rs`** — GDI-based tab bar rendering via `UpdateLayeredWindow`. Hit testing, hover tracking. Overlays are per-group, created lazily.
- **`drag.rs`** — Drag-and-drop state machine with 5px threshold. Drop targets: overlay (merge groups), managed window (create group), empty space (detach).
- **`tray.rs`** — System tray icon with Enable/Disable toggle and Exit.

### Key Patterns

- **RefCell re-entrancy safety**: `try_with_state()` exists because Win32 callbacks can re-enter during `with_state()` borrows. Never call Win32 APIs that pump messages while holding a `with_state()` borrow.
- **Overlay windows**: `WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST` — they float above grouped windows without stealing focus.
- **Tab rendering**: 32-bit ARGB bitmap with `premultiply_pixel()` for alpha blending. Tab height 28px, icon 16x16.

## Testing

38 unit tests covering pure logic only (no Windows API calls). Tests are `#[cfg(test)]` modules inline in `drag.rs`, `group.rs`, and `overlay.rs`. The `group.rs` tests use `HWND(1)`, `HWND(2)` etc. as mock handles.

## Platform

Windows only. Sole dependency is `windows-sys 0.59` with specific Win32 feature gates listed in `Cargo.toml`.
