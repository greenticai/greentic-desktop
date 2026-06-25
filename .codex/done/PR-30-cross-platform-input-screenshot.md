PR-30 — Cross-Platform Input and Screenshot Backend

Goal: centralize keyboard, mouse and screenshot capture behind a safe API.

The vision fallback PR already requires screenshots before/after actions, annotated regions and confidence scores. This PR should provide those primitives for every platform.

Capabilities
input.move_mouse
input.click
input.double_click
input.drag
input.type_text
input.hotkey
screen.screenshot
screen.region_screenshot
screen.locate_text
screen.locate_image
OS backends
OS	Backend
Windows	UI Automation + Win32 input + screenshot
macOS	CoreGraphics + Accessibility permission
Linux X11	XTest/X11 screenshot
Linux Wayland	portals/compositor-specific/limited mode
Acceptance criteria
Same input API works across supported platforms.
Screenshot capture is permission-aware.
Evidence store receives consistent screenshots regardless of OS.
Wayland limitations are surfaced as capability failures, not runtime surprises.