PR-29 — Cross-Platform App Launcher and Window Manager

Goal: normalize app/window operations across Windows/macOS/Linux.

The existing Windows PR already has concepts like windows.open_app, windows.find_window, and windows.close_app. This PR should lift those into generic capabilities:

desktop.open_app
desktop.find_window
desktop.activate_window
desktop.list_windows
desktop.close_window
desktop.window_screenshot
Why separate this?

Because recording/replay should not care whether an app was opened via:

OS	App launch method
Windows	executable path, Start menu, PowerShell
macOS	bundle ID, open -a, app path
Linux	.desktop file, executable, Flatpak, Snap, AppImage
Acceptance criteria
Runner packages can declare app launch requirements in a portable way.
macOS app bundle IDs are supported.
Linux .desktop entries are supported.
Windows behaviour remains compatible.
Replay can restore the target app/window before executing steps.