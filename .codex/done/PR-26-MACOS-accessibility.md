PR-26 — macOS Accessibility Adapter

Goal: support native macOS desktop automation through Apple Accessibility APIs.

Suggested adapter ID
greentic.desktop.macos.ax
Technology options
macOS Accessibility API: AXUIElement
CoreGraphics for mouse/keyboard events and screenshots
AppleScript / Shortcuts fallback for selected apps
Vision fallback when AX metadata is weak
Capabilities
macos.find_app
macos.find_window
macos.read_window_tree
macos.find_element
macos.click_element
macos.type_text
macos.read_text
macos.assert_visible
macos.screenshot
macos.activate_app
macos.close_app
Locator strategy
target:
  preferred:
    ax_identifier: "customerEmail"
  fallback:
    ax_title: "Email"
    ax_role: "AXTextField"
  visual_fallback:
    nearby_text: "Email"
    region: center
Important macOS-specific work

This PR must handle permissions cleanly:

Accessibility permission
Screen Recording permission
Input Monitoring permission where needed
Clear “how to grant permissions” diagnostics
First-run permission checker
Acceptance criteria
Can open and activate a macOS app.
Can inspect an accessibility tree.
Can click a button by AX role/title/identifier.
Can type into a text field.
Can take evidence screenshots.
Gives a clear error if Accessibility or Screen Recording permission is missing.