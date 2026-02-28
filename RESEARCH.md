# WinTab Technical Research Report

## Windows APIs, Hooks, and OS Features for a Floating Tab System

**Date:** 2026-02-28
**Scope:** Comprehensive technical research for building a "floating tab" system that works on ANY window in Windows 10/11.

---

## Table of Contents

1. [Window Enumeration & Detection](#1-window-enumeration--detection)
2. [Window Hooks](#2-window-hooks)
3. [Window Manipulation](#3-window-manipulation)
4. [Overlay / Floating UI](#4-overlay--floating-ui)
5. [DWM (Desktop Window Manager)](#5-dwm-desktop-window-manager)
6. [Shell Integration](#6-shell-integration)
7. [Window Subclassing](#7-window-subclassing)
8. [Win32 vs UWP / WinUI](#8-win32-vs-uwp--winui)
9. [Accessibility APIs](#9-accessibility-apis)
10. [Existing Precedents](#10-existing-precedents)
11. [Recommended Architecture](#11-recommended-architecture)

---

## 1. Window Enumeration & Detection

### Core APIs

| API | Purpose | Header |
|-----|---------|--------|
| `EnumWindows` | Enumerate all top-level windows on the desktop | `winuser.h` |
| `EnumChildWindows` | Enumerate child windows of a given parent | `winuser.h` |
| `EnumThreadWindows` | Enumerate all nonchild windows for a specific thread | `winuser.h` |
| `EnumDesktopWindows` | Enumerate top-level windows on a specific desktop | `winuser.h` |
| `FindWindow` / `FindWindowEx` | Find a window by class name and/or title | `winuser.h` |
| `GetForegroundWindow` | Get the currently active foreground window | `winuser.h` |
| `GetWindow` | Traverse the window Z-order (GW_HWNDNEXT, GW_HWNDPREV) | `winuser.h` |

### EnumWindows Pattern

`EnumWindows` is the primary mechanism for discovering all top-level windows. It takes a callback function that receives each window handle:

```c
BOOL CALLBACK EnumWindowsProc(HWND hwnd, LPARAM lParam) {
    // Filter: must be visible, must have a title, must not be a tool window, etc.
    if (!IsWindowVisible(hwnd)) return TRUE; // skip, continue

    LONG exStyle = GetWindowLong(hwnd, GWL_EXSTYLE);
    if (exStyle & WS_EX_TOOLWINDOW) return TRUE; // skip tool windows

    HWND owner = GetWindow(hwnd, GW_OWNER);
    if (owner != NULL) return TRUE; // skip owned windows

    // This window is a candidate for tabbing
    // Store hwnd in the collection passed via lParam
    return TRUE;
}

EnumWindows(EnumWindowsProc, (LPARAM)&windowList);
```

### Window Filtering Criteria (What Makes a "Tabbable" Window)

Not all top-level windows should get tabs. Production tools like WindowTabs use a multi-stage filter:

**Stage 1 -- Basic Validation:**
- Window must exist: `IsWindow(hwnd)` returns TRUE
- Window must be visible or minimized: `IsWindowVisible(hwnd)` or `IsIconic(hwnd)`
- Must have the `WS_OVERLAPPEDWINDOW` style (standard app windows)
- Must NOT have `WS_EX_TOOLWINDOW` style (floating toolbars, palettes)

**Stage 2 -- Ownership and Parentage:**
- Owner window must be NULL or zero-sized (reject owned dialogs, property sheets)
- Must not be a child window (`WS_CHILD` style absent)

**Stage 3 -- DWM Cloaking Check (Windows 8+):**
```c
BOOL isCloaked = FALSE;
DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &isCloaked, sizeof(isCloaked));
if (isCloaked) // skip -- window is hidden by DWM (e.g., on another virtual desktop)
```

**Stage 4 -- Process-Level Rules:**
- Resolve the process path via `GetWindowThreadProcessId` + `OpenProcess` + `QueryFullProcessImageName`
- Check against blacklist (e.g., `taskmgr.exe`, your own process)
- Check against user-defined inclusion/exclusion rules

### Key Properties to Query

| API | What It Returns |
|-----|----------------|
| `GetWindowText` | Window title bar text |
| `GetClassName` | Win32 window class name (e.g., `CabinetWClass` for Explorer) |
| `GetWindowThreadProcessId` | Owning process ID and thread ID |
| `GetWindowLong(GWL_STYLE)` | Window style flags (`WS_OVERLAPPED`, `WS_POPUP`, `WS_CHILD`, etc.) |
| `GetWindowLong(GWL_EXSTYLE)` | Extended style flags (`WS_EX_TOOLWINDOW`, `WS_EX_TOPMOST`, etc.) |
| `GetWindowRect` | Screen-coordinate bounding rectangle |
| `GetWindowPlacement` | Min/max/restored positions plus show state |
| `IsIconic` | Whether window is minimized |
| `IsZoomed` | Whether window is maximized |

### Important Note for Windows 8+

`EnumWindows` on Windows 8+ only enumerates top-level windows of **desktop apps**. UWP/Store apps have a different windowing model but still appear as `ApplicationFrameWindow` class windows that ARE enumerated. The actual content is a child `Windows.UI.Core.CoreWindow`.

---

## 2. Window Hooks

### Two Distinct Hook Mechanisms

Windows provides two fundamentally different hook systems. Understanding which to use (and when to combine them) is critical:

#### A. SetWindowsHookEx -- Classic Message Hooks

Installs a hook procedure into a hook chain. The hook procedure can intercept messages **before** they reach the target window procedure.

```c
HHOOK SetWindowsHookEx(
    int       idHook,      // Hook type
    HOOKPROC  lpfn,        // Hook procedure
    HINSTANCE hmod,        // DLL module handle (NULL for thread-local)
    DWORD     dwThreadId   // 0 for global, or specific thread
);
```

**Critical constraint:** For **global hooks** (monitoring all threads/processes), the hook procedure MUST reside in a DLL. The system automatically loads this DLL into every target process. This means:
- You need a separate DLL project for hook procedures
- A 32-bit DLL can only hook 32-bit processes; a 64-bit DLL can only hook 64-bit processes
- For full coverage, you need BOTH a 32-bit and 64-bit DLL

**Relevant hook types for a tab system:**

| Hook Type | Purpose | Requires DLL? |
|-----------|---------|---------------|
| `WH_SHELL` | Window creation, destruction, activation, app commands | Yes (for global) |
| `WH_CBT` | Window create, destroy, activate, move, size, minimize, maximize | Yes (for global) |
| `WH_CALLWNDPROC` | Intercept messages BEFORE the target window procedure | Yes (for global) |
| `WH_CALLWNDPROCRET` | Intercept messages AFTER the target window procedure | Yes (for global) |
| `WH_KEYBOARD_LL` | Low-level keyboard input (global hotkeys) | No (runs in installer's thread) |
| `WH_MOUSE_LL` | Low-level mouse input | No (runs in installer's thread) |

**WH_SHELL notifications:**
- `HSHELL_WINDOWCREATED` -- a top-level window was created
- `HSHELL_WINDOWDESTROYED` -- a top-level window was destroyed
- `HSHELL_WINDOWACTIVATED` -- a different window was activated
- `HSHELL_REDRAW` -- a window's title changed
- `HSHELL_GETMINRECT` -- a window is being minimized/maximized

**WH_CBT notifications:**
- `HCBT_CREATEWND` -- a window is about to be created
- `HCBT_DESTROYWND` -- a window is about to be destroyed
- `HCBT_ACTIVATE` -- a window is about to be activated
- `HCBT_MOVESIZE` -- a window is about to be moved/sized
- `HCBT_MINMAX` -- a window is about to be minimized/maximized

#### B. SetWinEventHook -- Accessibility Event Hooks (RECOMMENDED PRIMARY APPROACH)

This is the mechanism used by **PowerToys FancyZones**, **WindowTabs**, and most modern window management tools. It is simpler, does not require DLL injection, and provides richer events.

```c
HWINEVENTHOOK SetWinEventHook(
    DWORD        eventMin,       // Minimum event value
    DWORD        eventMax,       // Maximum event value
    HMODULE      hmodWinEventProc, // NULL for out-of-context
    WINEVENTPROC pfnWinEventProc,  // Callback function
    DWORD        idProcess,      // 0 for all processes
    DWORD        idThread,       // 0 for all threads
    DWORD        dwFlags         // WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS
);
```

**Key advantage:** With `WINEVENT_OUTOFCONTEXT`, the callback runs in YOUR process. No DLL injection needed. No 32/64-bit split needed.

**Essential events for a tab system:**

| Event | Constant | Purpose |
|-------|----------|---------|
| `EVENT_OBJECT_CREATE` | `0x8000` | A window/object was created |
| `EVENT_OBJECT_DESTROY` | `0x8001` | A window/object was destroyed |
| `EVENT_OBJECT_SHOW` | `0x8002` | A window/object became visible |
| `EVENT_OBJECT_HIDE` | `0x8003` | A window/object was hidden |
| `EVENT_OBJECT_FOCUS` | `0x8005` | An object received keyboard focus |
| `EVENT_OBJECT_LOCATIONCHANGE` | `0x800B` | A window moved/resized |
| `EVENT_OBJECT_NAMECHANGE` | `0x800C` | A window's title text changed |
| `EVENT_OBJECT_UNCLOAKED` | `0x8018` | A window was uncloaked (DWM) |
| `EVENT_SYSTEM_FOREGROUND` | `0x0003` | The foreground window changed |
| `EVENT_SYSTEM_MOVESIZESTART` | `0x000A` | User started moving/sizing a window |
| `EVENT_SYSTEM_MOVESIZEEND` | `0x000B` | User finished moving/sizing a window |
| `EVENT_SYSTEM_MINIMIZESTART` | `0x0016` | A window is being minimized |
| `EVENT_SYSTEM_MINIMIZEEND` | `0x0017` | A window was restored from minimized |

**Callback signature:**
```c
void CALLBACK WinEventProc(
    HWINEVENTHOOK hWinEventHook,
    DWORD event,
    HWND hwnd,
    LONG idObject,
    LONG idChild,
    DWORD idEventThread,
    DWORD dwmsEventTime
) {
    // Filter: only care about top-level windows
    if (idObject != OBJID_WINDOW || idChild != CHILDID_SELF) return;

    switch (event) {
        case EVENT_OBJECT_CREATE:
            // New window appeared -- evaluate for tabbing
            break;
        case EVENT_OBJECT_DESTROY:
            // Window gone -- remove tab if managed
            break;
        case EVENT_OBJECT_LOCATIONCHANGE:
            // Window moved/resized -- reposition attached tab
            break;
        case EVENT_OBJECT_NAMECHANGE:
            // Title changed -- update tab text
            break;
        case EVENT_SYSTEM_FOREGROUND:
            // Focus changed -- update active/inactive tab opacity
            break;
    }
}
```

**IMPORTANT:** The calling thread MUST have a message loop for out-of-context hooks to work.

#### C. RegisterShellHookWindow -- Lightweight Alternative for Shell Events

An alternative to `SetWindowsHookEx(WH_SHELL, ...)` that does NOT require a DLL. Shell events arrive as window messages to your window procedure:

```c
// Register
RegisterShellHookWindow(hwndMyApp);
UINT WM_SHELLHOOK = RegisterWindowMessage(L"SHELLHOOK");

// In WndProc:
case WM_SHELLHOOK:
    switch (wParam) {
        case HSHELL_WINDOWCREATED:   // lParam = HWND of new window
        case HSHELL_WINDOWDESTROYED: // lParam = HWND of destroyed window
        case HSHELL_WINDOWACTIVATED: // lParam = HWND of activated window
        case HSHELL_REDRAW:          // lParam = HWND whose title changed
    }
```

### Recommended Hook Strategy for WinTab

Use a **layered approach:**

1. **Primary:** `SetWinEventHook` with `WINEVENT_OUTOFCONTEXT` for all window lifecycle and position tracking. This is what FancyZones and WindowTabs use. No DLL injection, no bitness issues.

2. **Supplementary:** `RegisterShellHookWindow` for reliable `HSHELL_WINDOWCREATED` / `HSHELL_WINDOWDESTROYED` notifications (these are slightly more reliable for top-level window lifecycle than `EVENT_OBJECT_CREATE`/`EVENT_OBJECT_DESTROY` which fire for ALL objects, not just windows).

3. **Keyboard hooks:** `SetWindowsHookEx(WH_KEYBOARD_LL, ...)` for global hotkeys (this runs in the installer's thread, no DLL needed). Alternatively, use `RegisterHotKey` for simpler hotkey registration.

4. **Startup enumeration:** `EnumWindows` on launch to discover all pre-existing windows.

---

## 3. Window Manipulation

### Positioning and Sizing

| API | Purpose |
|-----|---------|
| `SetWindowPos` | Change position, size, Z-order, and show state of a single window |
| `MoveWindow` | Simpler position+size change (no Z-order control) |
| `BeginDeferWindowPos` / `DeferWindowPos` / `EndDeferWindowPos` | Batch-move multiple windows atomically |
| `GetWindowRect` | Get current screen-coordinate rectangle |
| `GetWindowPlacement` / `SetWindowPlacement` | Get/set min/max/restored state and positions |
| `ShowWindow` | Show, hide, minimize, maximize, restore |
| `SetForegroundWindow` | Bring a window to the foreground |
| `BringWindowToTop` | Bring window to top of Z-order |

### SetWindowPos -- The Workhorse

```c
SetWindowPos(
    hwnd,              // Target window
    HWND_TOP,          // Z-order (HWND_TOP, HWND_TOPMOST, HWND_NOTOPMOST, etc.)
    x, y, cx, cy,     // Position and size
    SWP_NOACTIVATE |   // Don't activate the window
    SWP_NOZORDER |     // Don't change Z-order
    SWP_SHOWWINDOW     // Show the window
);
```

**Key flags for tab management:**
- `SWP_NOACTIVATE` -- reposition without stealing focus (critical for showing/hiding grouped windows)
- `SWP_NOZORDER` -- don't change Z-order when just repositioning
- `SWP_NOMOVE` / `SWP_NOSIZE` -- change only what you need
- `SWP_HIDEWINDOW` / `SWP_SHOWWINDOW` -- combine with position changes

### Batch Window Operations (DeferWindowPos)

When switching tabs in a group, you need to hide one window and show another at the same position. `DeferWindowPos` does this atomically to prevent flicker:

```c
HDWP hdwp = BeginDeferWindowPos(2);
hdwp = DeferWindowPos(hdwp, hwndOldActive, NULL, 0, 0, 0, 0,
    SWP_HIDEWINDOW | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
hdwp = DeferWindowPos(hdwp, hwndNewActive, HWND_TOP, x, y, cx, cy,
    SWP_SHOWWINDOW);
EndDeferWindowPos(hdwp);
```

**Constraint:** All windows in a `DeferWindowPos` batch must have the same parent. For top-level windows, the parent is the desktop, so this works.

### Showing/Hiding Windows in a Tab Group

Two approaches for hiding inactive tabs:

**Approach A: `ShowWindow(SW_HIDE)` / `ShowWindow(SW_SHOW)`**
- Simple and reliable
- The hidden window disappears from the taskbar
- The hidden window is not rendered by DWM

**Approach B: Move off-screen or set zero size**
- Window remains "visible" to the system
- Taskbar entry persists
- More complex but preserves some window state

**Recommendation:** Use `ShowWindow(SW_HIDE/SW_SHOW)` as WindowTabs does. Manage taskbar representation separately if needed.

### Window Properties API (Metadata Storage)

FancyZones stores per-window metadata using Window Properties, which is an excellent pattern for tab systems:

```c
// Attach custom data to any window (even foreign windows)
SetProp(hwnd, L"WinTab_GroupId", (HANDLE)(UINT_PTR)groupId);
SetProp(hwnd, L"WinTab_TabIndex", (HANDLE)(UINT_PTR)tabIndex);

// Read it back
DWORD groupId = (DWORD)(UINT_PTR)GetProp(hwnd, L"WinTab_GroupId");

// Clean up when done
RemoveProp(hwnd, L"WinTab_GroupId");
```

Window properties work cross-process and persist until the window is destroyed or the property is removed.

---

## 4. Overlay / Floating UI

### Creating the Tab Strip Window

The tab bar itself is a window owned by your process, positioned above the target window's title bar. Key extended styles:

```c
HWND hwndTab = CreateWindowEx(
    WS_EX_TOOLWINDOW |    // Don't show in taskbar or Alt+Tab
    WS_EX_LAYERED |       // Support per-pixel alpha transparency
    WS_EX_TOPMOST |       // Stay above other windows (OPTIONAL -- see discussion)
    WS_EX_NOACTIVATE,     // Don't steal focus when clicked (IMPORTANT)
    L"WinTabStrip",       // Your registered window class
    L"",                  // No title
    WS_POPUP,             // No border, no title bar
    x, y, width, height,
    NULL,                 // No parent (top-level)
    NULL,                 // No menu
    hInstance,
    NULL
);
```

### Extended Style Breakdown

| Style | Purpose | Notes |
|-------|---------|-------|
| `WS_EX_TOOLWINDOW` | Hides from taskbar and Alt+Tab | Essential -- tabs should be invisible to task switching |
| `WS_EX_LAYERED` | Enables per-pixel alpha and transparency | Required for semi-transparent tabs, smooth rounded corners |
| `WS_EX_TOPMOST` | Always on top of non-topmost windows | Use cautiously -- can interfere with other apps. Consider managing Z-order manually instead |
| `WS_EX_NOACTIVATE` | Clicking does not activate/focus the window | Critical -- clicking a tab should not steal focus from the managed window |
| `WS_EX_TRANSPARENT` | Click-through (mouse events pass to windows below) | NOT what you want for tabs (you need clicks), but useful for decorative overlays |

### WS_EX_NOACTIVATE -- Critical for Tab Clicks

When the user clicks a tab, you do NOT want the tab strip window to steal focus from the application window. `WS_EX_NOACTIVATE` prevents this. However, you must ALSO handle `WM_MOUSEACTIVATE` to return `MA_NOACTIVATE`:

```c
case WM_MOUSEACTIVATE:
    return MA_NOACTIVATE;
```

**Known issue:** If any other window from your process (e.g., the settings dialog) has focus, `WS_EX_NOACTIVATE` windows from the same process may start behaving normally (accepting activation). Handle this edge case by restoring focus to the managed window after tab clicks.

### Layered Window Rendering

For `WS_EX_LAYERED` windows, you have two rendering approaches:

**Approach A: `SetLayeredWindowAttributes`** -- Simple color-key or uniform alpha:
```c
SetLayeredWindowAttributes(hwndTab, 0, alpha, LWA_ALPHA);
```

**Approach B: `UpdateLayeredWindow`** -- Per-pixel alpha via a DIB section (more powerful):
```c
BLENDFUNCTION blend = { AC_SRC_OVER, 0, 255, AC_SRC_ALPHA };
UpdateLayeredWindow(hwndTab, hdcScreen, &ptDst, &size, hdcMem, &ptSrc, 0, &blend, ULW_ALPHA);
```

**Recommendation:** Use `UpdateLayeredWindow` with a Direct2D or GDI+ rendered bitmap for smooth, anti-aliased tab visuals with per-pixel transparency.

### Positioning the Tab Strip Relative to the Target Window

The tab strip must track the target window's position. Use `SetWinEventHook` with `EVENT_OBJECT_LOCATIONCHANGE`:

```c
void RepositionTabStrip(HWND hwndTarget, HWND hwndTabStrip) {
    RECT rc;
    GetWindowRect(hwndTarget, &rc);

    int tabHeight = 30; // pixels
    SetWindowPos(hwndTabStrip, NULL,
        rc.left, rc.top - tabHeight,   // Position above the title bar
        rc.right - rc.left, tabHeight, // Same width as target
        SWP_NOZORDER | SWP_NOACTIVATE);
}
```

### Z-Order Management

`WS_EX_TOPMOST` keeps the tab always visible but creates problems with fullscreen apps and other topmost windows. A better approach:

**Use `SetWindowPos` to place the tab strip just above the target window in Z-order:**
```c
SetWindowPos(hwndTabStrip, hwndTarget, 0, 0, 0, 0,
    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
// hwndTabStrip is now inserted AFTER (above) hwndTarget in Z-order
```

Then, whenever the target window's Z-order changes (detected via `EVENT_SYSTEM_FOREGROUND` or `EVENT_OBJECT_REORDER`), reposition the tab strip above it again.

**Trade-off:** This approach means tabs may be obscured by overlapping windows, but it plays nicely with the OS. WS_EX_TOPMOST should only be used if the user explicitly enables a "tabs always visible" option.

---

## 5. DWM (Desktop Window Manager)

### DWM Thumbnail API -- Live Window Previews

The DWM thumbnail API is the correct mechanism for implementing "hover over tab to preview window" functionality. It provides live, GPU-composited previews with zero CPU overhead for rendering.

**Key APIs:**

| API | Purpose |
|-----|---------|
| `DwmRegisterThumbnail` | Create a relationship between source and destination windows |
| `DwmUpdateThumbnailProperties` | Set position, size, opacity, and visibility of the thumbnail |
| `DwmQueryThumbnailSourceSize` | Get the source window's native size |
| `DwmUnregisterThumbnail` | Destroy the thumbnail relationship |

**Usage pattern for tab preview:**
```c
HTHUMBNAIL thumbnail = NULL;

// Register: show hwndSource's content inside hwndPreviewPopup
HRESULT hr = DwmRegisterThumbnail(hwndPreviewPopup, hwndSource, &thumbnail);
if (SUCCEEDED(hr)) {
    // Query source size to maintain aspect ratio
    SIZE sourceSize;
    DwmQueryThumbnailSourceSize(thumbnail, &sourceSize);

    // Calculate destination rect maintaining aspect ratio
    RECT destRect = CalculatePreviewRect(sourceSize, maxPreviewWidth);

    DWM_THUMBNAIL_PROPERTIES props = {};
    props.dwFlags = DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY | DWM_TNP_SOURCECLIENTAREAONLY;
    props.rcDestination = destRect;
    props.fVisible = TRUE;
    props.opacity = 255;                    // Full opacity
    props.fSourceClientAreaOnly = FALSE;    // Include title bar in preview

    DwmUpdateThumbnailProperties(thumbnail, &props);
}

// When done (mouse leaves tab):
DwmUnregisterThumbnail(thumbnail);
```

**Constraints:**
- The destination window (your preview popup) must be a top-level window owned by your process
- The source window can be any top-level window (including other processes)
- Thumbnails are rendered in 2D (no Flip3D-style 3D effects)
- Thumbnails are live and continuously updated by the compositor -- no CPU cost to maintain
- The thumbnail renders inside your destination window, not as a floating element

### DWM Window Attributes

| Attribute | Purpose |
|-----------|---------|
| `DWMWA_CLOAKED` | Detect if a window is cloaked (hidden by DWM, e.g., on another virtual desktop) |
| `DWMWA_EXTENDED_FRAME_BOUNDS` | Get the actual visible frame bounds (excludes invisible resize borders) |
| `DWMWA_NCRENDERING_ENABLED` | Check if non-client area rendering is DWM-composed |

**`DWMWA_EXTENDED_FRAME_BOUNDS` is important:** On Windows 10/11, `GetWindowRect` returns a rect that includes invisible resize borders (about 7px on each side). For accurate tab positioning, use:

```c
RECT extendedBounds;
DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &extendedBounds, sizeof(extendedBounds));
// extendedBounds is the actual visible area
```

### DwmExtendFrameIntoClientArea (For Your Own Windows Only)

If you want your tab strip to have a glass/acrylic background that matches the OS frame aesthetic:

```c
MARGINS margins = { -1 }; // Extend frame into entire client area
DwmExtendFrameIntoClientArea(hwndTabStrip, &margins);
```

This only works on your own windows. You cannot extend the frame of foreign windows.

---

## 6. Shell Integration

### IVirtualDesktopManager (COM Interface)

Documented in `shobjidl_core.h`. Provides basic virtual desktop awareness:

```cpp
#include <shobjidl_core.h>

IVirtualDesktopManager* pVDM = nullptr;
CoCreateInstance(CLSID_VirtualDesktopManager, nullptr, CLSCTX_ALL,
    IID_PPV_ARGS(&pVDM));

// Check if a window is on the current virtual desktop
BOOL isOnCurrent = FALSE;
pVDM->IsWindowOnCurrentVirtualDesktop(hwnd, &isOnCurrent);

// Get desktop ID for a window
GUID desktopId;
pVDM->GetWindowDesktopId(hwnd, &desktopId);

// Move a window to a different desktop
pVDM->MoveWindowToDesktop(hwnd, targetDesktopId);
```

**Use cases for WinTab:**
- Skip windows on other virtual desktops during enumeration
- Keep tab groups together when switching desktops
- (V2+) Support tab groups spanning virtual desktops

### Undocumented Virtual Desktop APIs

The documented `IVirtualDesktopManager` is minimal. For advanced features (enumerate desktops, get desktop names, listen for desktop switches), there are undocumented COM interfaces:

- `IVirtualDesktopManagerInternal` -- More control over desktop management
- `IVirtualDesktopNotification` -- Notifications for desktop creation/destruction/switch
- `IApplicationView` / `IApplicationViewCollection` -- Per-window application view management

These are used by tools like [VirtualDesktop (Grabacr07)](https://github.com/Grabacr07/VirtualDesktop), but they break between Windows builds. **Not recommended for production** unless you can maintain per-build compatibility.

### Taskbar Integration

When windows in a tab group are hidden with `ShowWindow(SW_HIDE)`, they disappear from the taskbar. Options:

1. **Accept it** -- Only the active tab appears in the taskbar (simplest approach, used by WindowTabs)
2. **ITaskbarList3** -- Manipulate taskbar button grouping and thumbnails programmatically
3. **Shell_NotifyIcon** -- Not relevant (this is for system tray icons)

### RegisterShellHookWindow (Covered in Section 2)

A lightweight way to receive shell notifications without DLL injection.

---

## 7. Window Subclassing

### Can You Subclass Foreign Windows?

**Short answer: Not directly from your process. Not safely. Not recommended.**

`SetWindowSubclass` (the safe subclassing API in `commctrl.h`) only works within the same process. You cannot call it on a window belonging to another process.

The legacy `SetWindowLongPtr(hwnd, GWLP_WNDPROC, ...)` technically works cross-process in some scenarios but:
- It is undefined behavior for windows in other processes
- UIPI (User Interface Privilege Isolation) blocks it if the target is elevated
- DPI awareness mismatches cause problems
- It requires the replacement window procedure to exist in the target process's address space

### When DLL Injection IS Required

If you need to intercept messages sent TO a foreign window (e.g., intercepting `WM_NCCALCSIZE` to shrink the title bar and make room for tabs), you must inject a DLL into the target process and subclass from within.

**Methods for DLL injection:**
1. **`SetWindowsHookEx`** -- Install a global hook (e.g., `WH_CALLWNDPROC`). The system loads your DLL into every process with windows.
2. **`CreateRemoteThread` + `LoadLibrary`** -- More targeted but requires `PROCESS_CREATE_THREAD` access.
3. **AppInit_DLLs** -- Registry-based, but disabled with Secure Boot on Windows 8+.

**Architecture requirement:** You need both a 32-bit and 64-bit DLL for full process coverage.

### Recommendation for WinTab

**Avoid subclassing foreign windows.** The entire tab system can be built as an external overlay without injecting into other processes:
- Tab strip is your own window positioned above the target
- Position tracking via `SetWinEventHook` (no injection needed)
- Window manipulation via `SetWindowPos` / `ShowWindow` (cross-process, no injection needed)
- Title and icon queries via `GetWindowText` / `SendMessage(WM_GETICON)` (cross-process, no injection needed)

DLL injection adds immense complexity (32/64-bit DLLs, anti-virus false positives, UIPI restrictions, crash risk in foreign processes) for marginal benefit.

---

## 8. Win32 vs UWP / WinUI

### API Surface Comparison for Window Management

| Capability | Win32 | UWP | WinUI 3 (Windows App SDK) |
|-----------|-------|-----|---------------------------|
| Enumerate foreign windows | Full (`EnumWindows`) | Not possible (sandboxed) | Full (has Win32 HWND access) |
| Hook window events | Full (`SetWinEventHook`, `SetWindowsHookEx`) | Not possible | Full (Win32 interop) |
| Manipulate foreign windows | Full (`SetWindowPos`, `ShowWindow`) | Not possible | Full (Win32 interop) |
| Create overlay windows | Full control | Limited | Full (via HWND interop) |
| DWM thumbnails | Full | Not possible | Full (via Win32 interop) |
| System tray icon | Full | Not applicable | Full (via Win32 interop) |
| Run as background process | Yes | Limited (background task restrictions) | Yes |
| Per-pixel alpha | Full (`UpdateLayeredWindow`) | Built-in (XAML Composition) | Built-in (XAML Composition) |

### Verdict: Use Win32

**Win32 is the only viable choice** for a window management tool that manipulates foreign windows. UWP is explicitly sandboxed and cannot access other apps' windows. WinUI 3 can interop with Win32 but adds overhead for no benefit in this use case.

**Recommended stack:**
- **Core engine:** Pure Win32 / C++ (or Rust with `windows` crate)
- **Tab rendering:** Direct2D on layered windows (lightweight, hardware-accelerated)
- **Settings UI (optional):** Could use WinUI 3 or WPF for a modern-looking settings dialog, but the core must be Win32
- **Build:** CMake (C++) or Cargo (Rust)

### Why Not Electron/Chromium-Based?

Multrin (an archived Electron-based tab manager) proved that Electron's overhead is excessive for a utility that must be lightweight and always running. Each Chromium renderer process consumes 50-100MB+ RAM.

---

## 9. Accessibility APIs

### UI Automation (UIA)

Microsoft UI Automation is the modern accessibility framework (successor to MSAA). It provides:

**Relevant capabilities:**
- `IUIAutomation::AddAutomationEventHandler` -- Subscribe to UIA events on specific elements
- `IUIAutomation::AddPropertyChangedEventHandler` -- Watch for property changes (name, bounds, etc.)
- `IUIAutomationElement::get_CurrentBoundingRectangle` -- Get element bounds
- `IUIAutomationElement::get_CurrentName` -- Get accessible name
- `IUIAutomationElement::get_CurrentProcessId` -- Get owning process

**Advantages over raw Win32:**
- Works consistently across Win32, UWP, WPF, and WinForms apps
- Provides richer structural information (tree of UI elements)
- Better support for modern app frameworks

**Disadvantages:**
- Higher overhead than `SetWinEventHook` (UIA creates COM proxies)
- Overkill for window-level tracking (UIA is designed for element-level inspection)
- Can be slow for rapid position updates

### MSAA (Microsoft Active Accessibility) -- Legacy

MSAA provides `SetWinEventHook`, which is the SAME API discussed in Section 2. Despite being the "legacy" accessibility API, its event hook mechanism is the best tool for the job.

**Key events (same as Section 2):**
- `EVENT_OBJECT_LOCATIONCHANGE` -- window moved/resized
- `EVENT_OBJECT_NAMECHANGE` -- title changed
- `EVENT_OBJECT_SHOW` / `EVENT_OBJECT_HIDE` -- visibility changed
- `EVENT_SYSTEM_FOREGROUND` -- foreground window changed

### Recommendation

Use `SetWinEventHook` (MSAA event infrastructure) for all window tracking. Reserve UIA for specific tasks like:
- Extracting window icon when `WM_GETICON` fails
- Reading accessible names for windows that don't set `WM_GETTEXT`
- Future features like inspecting window content structure

---

## 10. Existing Precedents

### Stardock Groupy 2 (Commercial -- $7.99)

**The gold standard** for window tabbing on Windows.

**Known technical approach:**
- Uses DLL injection via shell extensions to inject UI elements into Explorer and other windows
- Hooks into the window frame/non-client area to draw tabs directly on the window chrome
- Cannot work with apps that have modified title bars (custom-drawn chrome)
- Deep integration with the Windows shell -- tabs appear integrated with the window itself rather than floating above
- Supports drag-and-drop between groups, auto-grouping rules, and tab persistence

**Limitations:**
- Breaks with some apps (especially those with custom title bars)
- Heavy injection approach can conflict with security software
- Closed source

### TidyTabs (Commercial -- Nurgo Software)

**Very similar concept to WinTab spec.**

**Approach:**
- External overlay windows positioned above the title bar
- Does NOT inject into foreign processes
- Uses `SetWinEventHook` for window tracking
- Tabs are semi-transparent and appear on hover
- Lighter weight than Groupy but less deeply integrated

### WindowTabs (Open Source -- F#/.NET)

**The closest open-source reference implementation.** Available on GitHub under `leafOfTree/WindowTabs`.

**Architecture (confirmed from source):**

1. **Window detection:** Shell hooks (`HSHELL_WINDOWCREATED`, `HSHELL_WINDOWDESTROYED`) + WinEvents (`EVENT_OBJECT_SHOW`, `EVENT_OBJECT_HIDE`) + `EnumWindows` on startup

2. **Window filtering:** Multi-stage pipeline:
   - Basic validation (must have `WS_OVERLAPPEDWINDOW`, not `WS_EX_TOOLWINDOW`)
   - Visibility check (visible or minimized)
   - Owner validation (no owned dialogs)
   - Process blacklist/whitelist

3. **Tab rendering:** Win32 layered windows (`TabStrip` class creates the visual container)

4. **Tab positioning:** `SetWinEventHook` with `EVENT_OBJECT_LOCATIONCHANGE` on managed windows

5. **DWM integration:** `SuperBarPlugin` for taskbar thumbnail management

6. **Event flow:** `Event Source -> Program.receive() -> updateAppWindows() -> FilterService -> ensureWindowIsGrouped() -> Desktop group -> TabStripDecorator -> TabStrip rendering`

7. **Group management:** `Desktop` class manages `WindowGroup` collections; `WindowGroup` handles tab switching via `switchWindow()`

8. **Hotkeys:** System-wide keyboard hooks for `Ctrl+Tab`, `Ctrl+1-9`, etc.

### PowerToys FancyZones (Open Source -- Microsoft)

**Not a tab manager**, but the best reference for window tracking and manipulation patterns.

**Architecture (confirmed from source):**

1. **Hooks:** `SetWinEventHook` in `FancyZonesApp::InitHooks()` subscribes to:
   - `EVENT_SYSTEM_MOVESIZESTART` / `EVENT_SYSTEM_MOVESIZEEND`
   - `EVENT_OBJECT_NAMECHANGE`
   - `EVENT_OBJECT_UNCLOAKED`
   - `EVENT_OBJECT_SHOW`
   - `EVENT_OBJECT_CREATE`
   - `EVENT_OBJECT_LOCATIONCHANGE`

2. **Event processing:** Hooked events -> translated to Windows messages -> consumed by `FancyZones::WndProc()` message loop

3. **Metadata:** Uses Window Properties API (`SetProp`/`GetProp`) to attach zone data to foreign windows

4. **Window detection:** `GetWindowPlacement()` to check maximize state; `MonitorFromWindow()` for multi-monitor support

5. **Keyboard:** Low-level keyboard hook for `Win+Arrow` interception

### Multrin (Archived -- Electron)

Built on Electron. Used `SetWindowPos` to reparent windows into a Chromium container. Archived in 2021 due to maintenance burden and performance issues. **Not a recommended approach** due to Electron overhead.

### AltSnap (Open Source -- C)

A window manipulation tool (drag windows with Alt+click). Uses:
- `SetWindowsHookEx(WH_MOUSE_LL, ...)` for mouse interception
- `SetWindowPos` for window manipulation
- Very lightweight C implementation

### Windows 11 Snap Groups (OS Feature)

Built into the OS. Uses internal shell APIs not exposed to third-party developers. Snap groups appear in taskbar hover and Alt+Tab. No public API for third-party tab management integration.

### Microsoft "Sets" (Cancelled)

Microsoft's own attempt at window tabbing (appeared in Windows 10 Insider builds 2017-2018). Was deeply integrated into the shell using `IApplicationView` / `IApplicationViewCollection` interfaces. Cancelled due to complexity and app compatibility issues. The concept lives on in File Explorer tabs (Windows 11 22H2+).

---

## 11. Recommended Architecture

Based on all research, here is the recommended architecture for WinTab:

### Core Design: External Overlay (No DLL Injection)

```
+--------------------------------------------------+
|  WinTab Process (single 64-bit executable)       |
|                                                   |
|  +--------------------------------------------+  |
|  | Window Tracker                              |  |
|  | - SetWinEventHook (WINEVENT_OUTOFCONTEXT)   |  |
|  | - RegisterShellHookWindow                    |  |
|  | - EnumWindows (startup scan)                 |  |
|  +--------------------------------------------+  |
|                    |                              |
|                    v                              |
|  +--------------------------------------------+  |
|  | Filter Service                              |  |
|  | - Style checks (WS_OVERLAPPEDWINDOW, etc.)  |  |
|  | - DWM cloaked check                         |  |
|  | - Process blacklist/whitelist                |  |
|  | - User rules engine                         |  |
|  +--------------------------------------------+  |
|                    |                              |
|                    v                              |
|  +--------------------------------------------+  |
|  | Group Manager                               |  |
|  | - Tab group lifecycle                       |  |
|  | - Auto-grouping rules                       |  |
|  | - Window show/hide (ShowWindow)             |  |
|  | - Position sync (SetWindowPos)              |  |
|  +--------------------------------------------+  |
|                    |                              |
|                    v                              |
|  +--------------------------------------------+  |
|  | Tab Strip Renderer                          |  |
|  | - Layered windows (WS_EX_LAYERED)           |  |
|  | - WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE      |  |
|  | - Direct2D or GDI+ rendering                |  |
|  | - DWM thumbnail previews                    |  |
|  +--------------------------------------------+  |
|                    |                              |
|  +--------------------------------------------+  |
|  | Input Handler                               |  |
|  | - Mouse capture for tab drag                |  |
|  | - RegisterHotKey for keyboard shortcuts      |  |
|  | - WH_KEYBOARD_LL for advanced hotkeys       |  |
|  +--------------------------------------------+  |
|                                                   |
+--------------------------------------------------+
```

### Why No DLL Injection?

| Factor | With DLL Injection | Without (External Overlay) |
|--------|-------------------|---------------------------|
| Complexity | High (32+64-bit DLLs, per-process state) | Low (single process) |
| Antivirus compatibility | Poor (injection triggers AV alerts) | Good |
| Stability | Risk of crashing foreign processes | Only own process at risk |
| UIPI/Elevated apps | Cannot inject into elevated processes | `SetWinEventHook` works on all |
| Visual integration | Can draw directly on window chrome | Tabs float above windows |
| Tab click handling | Native in-process events | WS_EX_NOACTIVATE + custom handling |
| Maintenance | Per-Windows-version compatibility | Stable across versions |

The trade-off is visual integration -- Groupy's injected tabs look native. But the reliability, simplicity, and maintainability of the external overlay approach is strongly preferred for a new project. TidyTabs and WindowTabs both prove this approach works well in production.

### Event Flow for Core Operations

**Window appears:**
1. `HSHELL_WINDOWCREATED` or `EVENT_OBJECT_SHOW` fires
2. Filter Service evaluates the window
3. If eligible: create a `TabStripDecorator`, subscribe to `EVENT_OBJECT_LOCATIONCHANGE` for that window's thread
4. If auto-group rule matches: add to existing group; otherwise create a standalone (hidden) tab

**User drags tab A onto tab B:**
1. Tab A's `TabStrip` enters drag mode (`SetCapture`)
2. On drop, hit-test against all other tab strips
3. If dropped on Tab B's strip: merge groups
4. Hide Tab A's window (`ShowWindow(SW_HIDE)`)
5. Position Tab A's window to match Tab B's group position (`SetWindowPos`)
6. Update the merged tab strip UI

**User clicks inactive tab:**
1. `WM_LBUTTONDOWN` on tab strip (received because WS_EX_NOACTIVATE does not prevent mouse messages, just activation)
2. `DeferWindowPos`: hide current active window, show clicked window at same position
3. `SetForegroundWindow` on the newly shown window
4. Update tab strip UI (highlight the new active tab)

**Active window moves/resizes:**
1. `EVENT_OBJECT_LOCATIONCHANGE` fires
2. `GetWindowRect` on the active window (use `DWMWA_EXTENDED_FRAME_BOUNDS` for accuracy)
3. `SetWindowPos` on the tab strip to reposition above the window
4. `SetWindowPos` on all hidden group members to sync their stored position (for when they become active)

### Technology Recommendations

| Component | Recommendation | Rationale |
|-----------|---------------|-----------|
| Language | C++ or Rust | Low overhead, direct Win32 access, no runtime dependency |
| Tab rendering | Direct2D on layered windows | Hardware-accelerated, per-pixel alpha, smooth |
| Window tracking | `SetWinEventHook` + `RegisterShellHookWindow` | No DLL injection, proven in production |
| Tab previews | DWM Thumbnail API | Zero-cost live previews, GPU-composited |
| Config format | JSON (or TOML) | Human-readable, easy to parse |
| Hotkeys | `RegisterHotKey` + `WH_KEYBOARD_LL` | RegisterHotKey for simple combos, LL hook for advanced |
| Position queries | `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)` | Accurate bounds excluding invisible resize borders |
| Metadata storage | Window Properties API (`SetProp`/`GetProp`) | Attach data to foreign windows without injection |

---

## Sources

### Microsoft Official Documentation
- [EnumWindows function](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-enumwindows)
- [SetWindowsHookExW function](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowshookexw)
- [SetWinEventHook function](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook)
- [Hooks Overview](https://learn.microsoft.com/en-us/windows/win32/winmsg/about-hooks)
- [Using Hooks](https://learn.microsoft.com/en-us/windows/win32/winmsg/using-hooks)
- [SetWindowPos function](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos)
- [DeferWindowPos function](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-deferwindowpos)
- [Extended Window Styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles)
- [Window Features](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features)
- [DWM Thumbnail Overview](https://learn.microsoft.com/en-us/windows/win32/dwm/thumbnail-ovw)
- [DwmRegisterThumbnail function](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/nf-dwmapi-dwmregisterthumbnail)
- [DWMWINDOWATTRIBUTE enumeration](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute)
- [Custom Window Frame Using DWM](https://learn.microsoft.com/en-us/windows/win32/dwm/customframe)
- [IVirtualDesktopManager interface](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ivirtualdesktopmanager)
- [RegisterShellHookWindow function](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registershellhookwindow)
- [Subclassing Controls](https://learn.microsoft.com/en-us/windows/win32/controls/subclassing-overview)
- [SetParent function](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setparent)
- [GetWindowRect function](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowrect)
- [UI Automation and Active Accessibility](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-msaa)

### Technical Articles and Blogs
- [How FancyZones Works -- Sam Rambles](https://samrambles.com/guides/fancyzones/how-fancyzones-works/index.html)
- [How can I write a program that monitors another window for a change in size or position? -- The Old New Thing (Raymond Chen)](https://devblogs.microsoft.com/oldnewthing/20210104-00/?p=104656)
- [Implementing Global Injection and Hooking in Windows -- m417z](https://m417z.com/Implementing-Global-Injection-and-Hooking-in-Windows/)
- [Windows Hook Events -- Pavel Yosifovich](https://scorpiosoftware.net/2023/09/24/windows-hook-events/)
- [Custom Window Title Bar -- Dmitriy Kubyshkin](https://kubyshkin.name/posts/win32-window-custom-title-bar-caption/)
- [.NET Interception of External Window Messages -- Bad Echo](https://badecho.com/index.php/2024/01/13/external-window-messages/)
- [WindowTabs Architecture -- DeepWiki](https://deepwiki.com/leafOfTree/WindowTabs)

### Open Source Projects
- [WindowTabs (leafOfTree)](https://github.com/leafOfTree/WindowTabs) -- Active F#/.NET tab manager
- [WindowTabs (standard-software)](https://github.com/standard-software/WindowTabs) -- Fork with additional features
- [PowerToys (Microsoft)](https://github.com/microsoft/PowerToys) -- FancyZones source code
- [Multrin (sentialx)](https://github.com/sentialx/multrin) -- Archived Electron-based tab manager
- [VirtualDesktop (Grabacr07)](https://github.com/Grabacr07/VirtualDesktop) -- Virtual desktop COM wrapper
- [VirtualDesktop (MScholtes)](https://github.com/MScholtes/VirtualDesktop) -- Virtual desktop management tool

### Commercial Products
- [Stardock Groupy 2](https://www.stardock.com/products/groupy/)
- [TidyTabs (Nurgo Software)](https://www.nurgo-software.com/products/tidytabs)
