# PR-95 - Real Linux X11 and Wayland Replay and Recording

## Goal

Replace Linux in-memory adapters with real X11/AT-SPI/XTest implementations and honest Wayland portal/AT-SPI constrained behavior.

## User Outcome

On Linux X11, Greentic can drive real GTK/Qt apps, type/click/read UI state, and record real interactions. On Wayland, Greentic only advertises capabilities the compositor/portal actually allows.

## Current Evidence

- `LinuxX11Adapter::execute` mutates `LinuxState`.
- `LinuxWaylandAdapter::execute` mutates `WaylandState`.
- Recording backends emit synthetic initial events.

## Scope

1. Implement X11 backend:
   - window discovery via X11/EWMH or `x11rb`
   - AT-SPI tree traversal via `atspi`
   - input through XTest where permitted
   - screenshots via X11 capture or portal where configured
2. Implement Wayland backend honestly:
   - AT-SPI read/metadata where available
   - xdg-desktop-portal screenshots with user approval
   - safe keyboard shortcuts only where supported
   - no global click/type/window automation unless compositor explicitly allows it
3. Implement:
   - `linux.find_window`
   - `linux.activate_window`
   - `linux.read_window_tree`
   - `linux.find_element`
   - `linux.type_text`
   - `linux.click_element`
   - `linux.read_text`
   - `linux.assert_visible`
   - `linux.screenshot`
   - `linux.close_window`
   - Wayland-specific constrained capabilities
4. Implement recording through AT-SPI events and window focus listeners.
5. Persist AT-SPI trees, screenshots, and portal denial evidence.

## E2E Fixtures

1. GTK fixture app.
2. Qt fixture app.
3. Headed Xvfb X11 CI path.
4. Wayland contract tests that prove unsupported global automation is not advertised.

## Acceptance Tests

1. Xvfb X11 E2E creates a real file via fixture app.
2. AT-SPI tree inspection returns real fixture controls.
3. XTest input is only used after target window/control resolution.
4. Wayland without portal/AT-SPI blocks with actionable diagnostics.
5. Wayland never claims full desktop automation when compositor forbids it.

