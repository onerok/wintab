# find_peek_candidate lacks z-order awareness

## Location

`src/state.rs:324-353` — `AppState::find_peek_candidate`

## Problem

After a group is created, the foreground window is the active tab of the group. `find_peek_candidate` correctly skips it (grouped), but the fallback loop iterates `self.windows.keys()` (HashMap, arbitrary order) and checks `cursor_in_hot_zone` purely by geometry — it does not verify the window is actually visible under the cursor in the z-order.

This means peek can appear for a window that is fully behind the grouped window if their top edges overlap vertically. The user sees a peek tab for a window they can't see, which feels broken.

## Reproduction

1. Create a group {A, B} so B is active and foreground
2. Position ungrouped window C so its top edge is behind B
3. Hover near the shared top edge — peek appears for C even though C is occluded by B

## Suggested Fix

Use z-order-aware enumeration instead of scanning all windows. Options:

1. **`WindowFromPoint` approach**: Call `WindowFromPoint` at a representative point in the hot zone. If the returned HWND is an ungrouped managed window, use it. This naturally respects z-order.

2. **`EnumWindows` z-order scan**: Enumerate top-level windows in z-order (front to back). For each, check if it's managed, ungrouped, not minimized, and the cursor is in its hot zone. Return the first match. Stop scanning once a managed window is found even if it's grouped (it occludes anything behind it).

Option 1 is simpler but only checks a single point. Option 2 is more thorough.
