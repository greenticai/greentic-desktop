# Adapters and Supported Desktops

Adapters are the bridge between a runner and a real application. A runner asks for capabilities such as `web.click`, `terminal.read_screen`, or `macos.type_text`; adapters provide those capabilities.

## Built-In Extension Manifests

The current repository includes built-in manifests for:

| Extension ID | Use Case |
| --- | --- |
| `greentic.desktop.playwright` | Browser and web app automation. |
| `greentic.desktop.terminal-tn3270` | Terminal and mainframe-style screen automation. |
| `greentic.desktop.windows-ui` | Windows UI Automation for native Windows apps. |
| `greentic.desktop.java-accessibility` | Java desktop apps through accessibility metadata. |
| `greentic.desktop.vision` | Screenshot and visual fallback when structured UI access is limited. |
| `greentic.desktop.macos.ax` | macOS Accessibility automation. |
| `greentic.desktop.linux.x11` | Linux X11 window and UI automation. |
| `greentic.desktop.linux.wayland` | Constrained Wayland compatibility using portal screenshots, accessibility trees, and safe shortcuts. |

## Web Apps

The Playwright adapter model supports browser actions such as:

- going to a URL,
- clicking and filling fields,
- selecting values,
- waiting for text,
- checking visibility or URL,
- extracting text or regex matches,
- taking screenshots,
- downloading files.

## Native Desktop Apps

Native desktop adapters use platform APIs where possible:

- Windows uses UI Automation concepts.
- macOS uses Accessibility-style app, window, and element operations.
- Linux X11 uses window management, UI tree access, screenshots, and input.
- Linux Wayland is intentionally more constrained and reports unsupported operations where the desktop environment does not allow safe automation.

## Terminal Systems

The terminal adapter model supports connecting to a terminal, reading the screen, sending keys or text, waiting for screen state, asserting text, extracting fields, and capturing screen evidence.

## Vision Fallback

Vision fallback is for cases where structured automation is not available. It works with screenshots, text or button search, region clicks, baseline comparisons, visual assertions, and extracted text.

Vision is useful as a backup path, not as the first choice when reliable structured locators are available.

## Permissions

Desktop adapters often need operating-system permissions. Common examples are accessibility access, screen recording or screenshots, input monitoring, app launch permission, and window management permission.

See [Security](security.md) for how policy controls what an adapter or runner is allowed to do.
