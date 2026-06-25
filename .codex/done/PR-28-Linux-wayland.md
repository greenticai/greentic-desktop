PR-28 — Linux Wayland Compatibility Layer

Goal: support Linux Wayland safely, while being honest about its restrictions.

Wayland intentionally restricts global screen capture, global input injection and window introspection. So this should not pretend to support everything. It should detect what is possible and route to approved mechanisms.

Suggested adapter ID
greentic.desktop.linux.wayland
Technology options
AT-SPI2 where available
xdg-desktop-portal for screenshots / screen capture
compositor-specific support where approved:
GNOME/Mutter
KDE/KWin
wlroots-based compositors
Keyboard shortcut fallback
Vision fallback only where screenshots are permitted
Capabilities
linux.wayland.detect
linux.wayland.portal_screenshot
linux.wayland.accessibility_tree
linux.wayland.assert_visible
linux.wayland.safe_keyboard_shortcut
Acceptance criteria
Detects Wayland vs X11.
Reports missing global automation capabilities clearly.
Uses xdg-desktop-portal for screenshots where possible.
Can automate accessible apps via AT-SPI where supported.
Falls back to “manual approval required” or “unsupported” instead of silently using unreliable coordinate hacks.

This is important because Wayland should be treated as a constrained desktop, not just “Linux X11 but newer”.