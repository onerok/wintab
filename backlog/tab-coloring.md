# Tab Coloring

## Problem

Tab colors are hard-coded constants in `overlay.rs` (`COLOR_ACTIVE = 0x00A06030`, `COLOR_INACTIVE = 0x00705040`, `COLOR_HOVER = 0x00C08050`). Users cannot customize the tab appearance to match their desktop theme or distinguish groups by color. The spec defines four color modes: System accent, Per-app, Custom, and None (neutral).

## Location

**Files modified:**

- `src/overlay.rs` — Replace hard-coded color constants with dynamic color resolution. Add `resolve_tab_color()` function that returns colors based on the active color mode. Modify `paint_tabs()` to use resolved colors.
- `src/config.rs` — Add color mode config fields: `tab_color_mode`, `custom_active_color`, `custom_inactive_color`.
- `src/state.rs` — Pass config to overlay rendering calls.
- `src/window.rs` — Add `dominant_icon_color()` helper for per-app color extraction (optional).

**New files:** None.

## Requirements

- [ ] Four color modes selectable via config:
  - **System accent** — reads the Windows accent color from `DwmGetColorizationColor` or the registry
  - **Per-app** — extracts the dominant color from each window's application icon
  - **Custom** — user-specified RRGGBB values for active and inactive tabs
  - **None** — neutral gray/white tabs
- [ ] Active, inactive, and hover colors are all derived from the base color mode.
- [ ] Color changes apply immediately to all existing overlays (live repaint).
- [ ] Default mode is System accent (matches Windows theme).
- [ ] Custom colors are stored in `config.yaml` as hex strings (e.g., `"#A06030"`).

## Suggested Implementation

### Config fields

```rust
#[derive(Clone, Deserialize, Serialize)]
pub enum TabColorMode {
    SystemAccent,
    PerApp,
    Custom,
    None,
}

// In config struct:
pub tab_color_mode: TabColorMode,       // default: SystemAccent
pub custom_active_color: u32,           // 0x00RRGGBB, default: 0x00A06030
pub custom_inactive_color: u32,         // 0x00RRGGBB, default: 0x00705040
```

### Color resolution

```rust
fn resolve_tab_colors(mode: &TabColorMode, config: &Config, icon: HICON) -> (u32, u32, u32) {
    match mode {
        TabColorMode::SystemAccent => {
            let accent = get_system_accent_color();
            let inactive = darken(accent, 0.6);
            let hover = lighten(accent, 1.2);
            (accent, inactive, hover)
        }
        TabColorMode::PerApp => {
            let base = extract_dominant_color(icon).unwrap_or(0x00808080);
            let inactive = darken(base, 0.6);
            let hover = lighten(base, 1.2);
            (base, inactive, hover)
        }
        TabColorMode::Custom => {
            let hover = lighten(config.custom_active_color, 1.3);
            (config.custom_active_color, config.custom_inactive_color, hover)
        }
        TabColorMode::None => {
            (0x00606060, 0x00404040, 0x00808080)
        }
    }
}
```

### System accent color

```rust
fn get_system_accent_color() -> u32 {
    unsafe {
        let mut colorization: u32 = 0;
        let mut opaque_blend: i32 = 0;
        // DwmGetColorizationColor returns AARRGGBB
        if DwmGetColorizationColor(&mut colorization, &mut opaque_blend) == 0 {
            colorization & 0x00FFFFFF // strip alpha
        } else {
            0x00A06030 // fallback
        }
    }
}
```

### Dominant icon color extraction

```rust
fn extract_dominant_color(icon: HICON) -> Option<u32> {
    // 1. Get icon bitmap via GetIconInfo
    // 2. Create a compatible DC, select bitmap
    // 3. Sample pixels, compute average RGB (skip transparent pixels)
    // 4. Return the average as 0x00RRGGBB
    // This is approximate — a simple average works well enough for tab coloring
}
```

### Color math helpers

```rust
fn darken(color: u32, factor: f32) -> u32 {
    let r = ((color >> 16) & 0xFF) as f32 * factor;
    let g = ((color >> 8) & 0xFF) as f32 * factor;
    let b = (color & 0xFF) as f32 * factor;
    ((r.min(255.0) as u32) << 16) | ((g.min(255.0) as u32) << 8) | (b.min(255.0) as u32)
}

fn lighten(color: u32, factor: f32) -> u32 {
    darken(color, factor) // same math, factor > 1.0 lightens
}
```

## Edge Cases

- **System accent changes**: Windows sends `WM_DWMCOLORIZATIONCOLORCHANGED` when the user changes their accent color in Settings. Handle this in the main message loop to trigger a full overlay repaint.

- **Per-app with no icon**: Some windows have no icon (`HICON` is null). Fall back to neutral gray for those tabs.

- **Per-app color similarity**: Two apps in the same group may have very similar icon colors, making tabs indistinguishable. Per-app mode is best suited for distinguishing groups, not tabs within a group.

- **High-contrast mode**: When Windows high-contrast mode is active, ignore the color mode and use system-defined high-contrast colors (`GetSysColor(COLOR_HIGHLIGHT)` etc.).

- **Alpha premultiplication**: Colors passed to `fill_rect_alpha` and `premultiply_pixel` must be in 0x00RRGGBB format without alpha. The alpha channel is applied separately during rendering.

- **Config migration**: Adding `tab_color_mode` to an existing `config.yaml` that lacks it should fall back to `SystemAccent` (the default), not error.

- **Performance of per-app extraction**: Icon color extraction involves bitmap manipulation. Cache the result per process name in a `HashMap<String, u32>` to avoid re-extracting on every repaint.
