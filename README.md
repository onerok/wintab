# WinTab

Browser-style tab grouping for any window on Windows 10/11.

WinTab adds a tab bar above grouped windows, letting you switch between them with a click. Drag tabs to rearrange groups, hover for a thumbnail preview, and color-code tabs to tell similar windows apart.

## Features

- **Tab grouping** — drag any window onto another to create a tab group
- **Auto-grouping** — rules in `config.yaml` automatically group windows by process, title, or class
- **Tab colors** — pattern-based color rules tint tabs so you can distinguish similar windows at a glance
- **Thumbnail preview** — hover a tab to see a live DWM thumbnail of the window
- **Drag and drop** — drag tabs between groups, onto windows, or into empty space to detach
- **Virtual desktop aware** — tab bars hide when their group is on another desktop
- **Position memory** — window positions and sizes are restored when groups re-form
- **System tray** — enable/disable from the tray icon
- **Hot-reload** — edit `config.yaml` and changes apply immediately

## Install

Requires Windows 10+ and [Rust](https://rustup.rs/).

```
cargo install --path .
```

Or build a release binary:

```
just release
```

The optimized binary is at `target/release/wintab.exe`.

## Usage

Run `wintab.exe`. It sits in the system tray and monitors windows automatically.

- **Group windows:** Drag one window's tab onto another window
- **Switch tabs:** Click a tab in the tab bar
- **Ungroup:** Drag a tab to empty space
- **Preview:** Hover over an inactive tab (500ms delay)
- **Disable/Exit:** Right-click the tray icon

## Configuration

Config file location: `%APPDATA%\WinTab\config.yaml`

Copy `config.sample.yaml` from this repo to get started:

```
mkdir "%APPDATA%\WinTab"
copy config.sample.yaml "%APPDATA%\WinTab\config.yaml"
```

Changes are hot-reloaded — no restart needed.

### Auto-grouping rules

Group windows automatically by matching process name, window title, class name, or command line:

```yaml
rules:
  - name: "VS Code"
    patterns:
      - field: process_name
        op: equals
        value: "Code.exe"

  - name: "Terminal"
    patterns:
      - field: process_name
        op: equals
        value: "WindowsTerminal.exe"
```

Multiple patterns with `match: all` (default) require every pattern to match. Use `match: any` for OR logic.

### Tab colors

Color-code tabs based on window metadata. Useful when many windows share the same process (e.g., multiple VS Code SSH sessions):

```yaml
tab_color_style: tint_stripe   # default style

tab_colors:
  - pattern:
      field: title
      op: contains
      value: "SSH: prod"
    color: "#CD5C5C"           # red for production

  - pattern:
      field: title
      op: contains
      value: "SSH: dev"
    color: "#2E8B57"           # green for development
```

**Styles:**

| Style | Description |
|---|---|
| `tint_stripe` | Tinted background + 2px bottom accent (default) |
| `bottom_stripe` | Default background + 3px colored bottom stripe |
| `top_stripe` | Default background + 3px colored top stripe |
| `left_bar` | Default background + 3px colored left bar |
| `full_tint` | Entire tab background uses the rule color |

### Pattern reference

**Fields:** `process_name`, `class_name`, `title`, `command_line`

**Operators:** `equals`, `contains`, `starts_with`, `ends_with`, `not_equals`, `not_contains`, `regex`

All matching is case-insensitive by default. Add `case_sensitive: true` to a pattern for exact casing.

### Preview settings

```yaml
preview:
  width: 300        # pixels (default: 300)
  max_height: 400   # pixels (default: 400)
  opacity: 200      # 0-255 (default: 200)
  delay_ms: 500     # hover delay in ms (default: 500)
```

## Development

Requires [just](https://github.com/casey/just) task runner.

```
just build          # debug build
just run            # build and run
just test           # unit + acceptance tests
just test-all       # full suite including desktop-switch E2E
just lint           # clippy with -D warnings
just fmt            # format code
```

## Architecture

Single-process, single-threaded Win32 application. No DLL injection — uses `SetWinEventHook` with `WINEVENT_OUTOFCONTEXT` to monitor window events. Overlays are per-group layered windows rendered with GDI + `UpdateLayeredWindow`. All state lives in a thread-local `RefCell<AppState>`.

See [CLAUDE.md](CLAUDE.md) for detailed architecture notes.
