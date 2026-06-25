PR-27 — Linux Desktop Adapter: X11 First

Goal: support Linux desktop automation where X11 is available.

Suggested adapter ID
greentic.desktop.linux.x11
Technology options
AT-SPI2 accessibility tree
X11 / XTest for input
xdotool fallback
wmctrl fallback for window management
xprop / xwininfo for metadata
Screenshot via X11 APIs or external tools
Vision fallback
Capabilities
linux.find_window
linux.read_window_tree
linux.find_element
linux.click_element
linux.type_text
linux.read_text
linux.assert_visible
linux.screenshot
linux.activate_window
linux.close_window
Locator strategy
target:
  preferred:
    accessible_name: "Customer Search"
    role: "text"
  fallback:
    window_title: "CRM"
    class_name: "GtkEntry"
  visual_fallback:
    nearby_text: "Customer"
    region: center
Acceptance criteria
Can detect X11 session.
Can list windows.
Can inspect AT-SPI accessibility tree.
Can click and type into GTK/Qt apps with accessible metadata.
Can use keyboard/mouse fallback where metadata is incomplete.
Can capture screenshots and audit evidence.

This should be X11 first, because it is much easier and more realistic for controlled enterprise desktops, VDI images, test workspaces and containers with virtual displays.