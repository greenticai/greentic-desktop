PR-25 — Cross-Platform Desktop Platform Layer

Goal: make the current runner explicitly OS-aware before adding more desktop adapters.

This PR should introduce a shared platform model:

pub enum DesktopPlatform {
    Windows,
    MacOS,
    Linux,
}

pub struct PlatformInfo {
    pub os: DesktopPlatform,
    pub version: String,
    pub desktop_environment: Option<String>,
    pub display_server: Option<String>, // x11, wayland, quartz, rdp, etc.
    pub permissions: Vec<PlatformPermission>,
}
What it should add
platform.detect
platform.permissions.check
platform.permissions.explain
platform.open_app
platform.activate_window
platform.list_windows
platform.screenshot
platform.input.keyboard
platform.input.mouse
Why this PR comes first

The existing adapter SDK wants the runner to select the best adapter automatically and fail unsupported steps before execution. Mac/Linux need platform capability detection because support depends heavily on:

Platform	Key issue
macOS	Accessibility permissions, Screen Recording permission, app sandboxing
Linux X11	Easier global input/screenshot/window control
Linux Wayland	Much more restricted automation and screenshot control
Remote desktops	Often only vision/keyboard fallback works reliably
Acceptance criteria
Runner can detect Windows/macOS/Linux.
Runner can list available platform capabilities.
Runner can reject a runner package if the current desktop cannot support its required capabilities.
Existing Windows roadmap remains compatible.