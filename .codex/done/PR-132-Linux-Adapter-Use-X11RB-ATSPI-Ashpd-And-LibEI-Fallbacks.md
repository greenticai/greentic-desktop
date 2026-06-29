# PR-132 - Linux Adapter Use X11RB ATSPI Ashpd And LibEI Fallbacks

## Goal

Implement Linux desktop automation through the correct display-server and accessibility APIs for X11 and Wayland.

## User Outcome

Linux users get clear support boundaries: X11 can automate through X11/AT-SPI, while Wayland uses portals or fails closed when global input is blocked by design.

## Current Evidence

- Linux behavior must split between X11 and Wayland.
- Wayland cannot be made equivalent to X11 without compositor/portal consent.
- The adapter should use libraries instead of shelling out to desktop tools.

## Scope

1. Add dependencies:
   - `x11rb` for X11 window management and XTest input where available.
   - `atspi` over `zbus` for accessibility tree traversal and events.
   - `ashpd` for XDG Desktop Portal RemoteDesktop and ScreenCast.
   - evaluate `reis`, `libei`, `ydotool`, `uinput`, and `evdev` as fallbacks with explicit trust/permission boundaries.
   - `xcap` for screenshot baseline where supported.
2. Implement X11 path:
   - enumerate windows.
   - activate/focus.
   - AT-SPI locator matching.
   - click/type/shortcut via XTest or enigo.
   - screenshot/evidence.
3. Implement Wayland path:
   - detect compositor/session.
   - request portal screen/cast/remote desktop permissions through `ashpd`.
   - support only approved actions.
   - fail closed for disallowed global input.
4. Add explicit capability matrix updates for X11 vs Wayland.

## File Targets

- `crates/greentic-desktop-linux/src/lib.rs`
- `crates/greentic-desktop-platform/src/lib.rs`
- `crates/greentic-desktop-workflow/src/lib.rs`
- `docs/adapters/linux-x11.md`
- `docs/adapters/linux-wayland.md`
- `docs/capability-matrix.md`

## Out of Scope

- Pretending Wayland can do unrestricted global input.
- Distro-specific shell scripts as primary automation.

## Acceptance Tests

1. X11 implementation uses `x11rb`/AT-SPI APIs rather than shelling out to `wmctrl`/`xdotool` for core primitives.
2. Wayland portal flow returns structured pending/denied/approved states.
3. AT-SPI locator tests cover role/name/description/value/bounds.
4. X11 fixture E2E proves open/input/save/read-output on a simple app.
5. Wayland tests prove graceful limitation instead of fake success.

## Done Means

Linux behavior is API-backed, display-server-aware, and honest about Wayland restrictions.
