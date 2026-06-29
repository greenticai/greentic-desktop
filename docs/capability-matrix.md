# Capability Matrix

This matrix is the source of truth for what Greentic Desktop can prove today.
Do not treat a capability as production-ready unless it has a passing fixture or
live end-to-end test that exercises the real adapter path and verifies the
declared output.

Status meanings:

- **Production**: real adapter execution is proven by end-to-end tests and is
  suitable for controlled rollout.
- **Beta**: real execution is implemented for a supported path, but rollout
  should remain limited and monitored.
- **Experimental**: the adapter or flow exists, but coverage, permissions,
  sidecars, or platform behavior are not yet sufficient for unattended
  production use.
- **Model-only**: the repository has schema, planning, or manifest support, but
  no production execution backend should claim desktop side effects.

| Area | Adapter or Transport | Status | Required Setup | Proven Coverage | Current Limits |
| --- | --- | --- | --- | --- | --- |
| Browser and web apps | `greentic.desktop.playwright` | Beta | Playwright/web adapter installed; Greentic-owned browser context for recording. | Web runner replay through the GUI-managed MCP path against a local fixture, typed Playwright sidecar request/response id checks, output persistence, and evidence files. | It records the controlled browser context Greentic opens, not arbitrary existing browser tabs. |
| macOS native apps | `greentic.desktop.macos.ax` | Experimental | Accessibility, input control, and screen recording permissions granted to the process running Greentic Desktop. | Capability preflight and generic primitive execution paths for app activation, shortcuts, menu commands, focus, text entry, and save-as actions. | Native app behavior depends on app accessibility metadata and visible user session state; fixture E2Es for common native app workflows are still required before production use. |
| Windows native apps | `greentic.desktop.windows-ui` | Experimental | Windows UI Automation access in an interactive desktop session. | Capability routing and adapter manifest coverage. | Real UIA replay and recording fixture coverage are not yet production-proven in this repository. |
| Linux X11 native apps | `greentic.desktop.linux.x11` | Experimental | X11 session, visible target window, AT-SPI/window metadata, and screenshot/input permissions where required. | Session detection and capability routing coverage. | Global window/input control depends on the desktop environment and target toolkit; production fixture E2Es are still required. |
| Linux Wayland native apps | `greentic.desktop.linux.wayland` | Experimental | Wayland session with portal screenshots or compositor-approved access. | Wayland detection and restricted-mode diagnostics. | Wayland intentionally blocks unrestricted global input/window control; unsupported actions must fail with a concrete reason. |
| Java desktop apps | `greentic.desktop.java-accessibility` | Experimental | Java Access Bridge or equivalent accessibility/event source for the Java runtime launching the target app. | Manifest, routing, and recording preflight coverage. | It is only for Java applications; it must not be selected for non-Java desktop apps such as Word. |
| Terminal and mainframe apps | `greentic.desktop.terminal-tn3270` | Experimental | Greentic-owned PTY, SSH, or TN3270 runtime configured with `GREENTIC_TERMINAL_ADAPTER_COMMAND`. | Terminal runner model and sidecar readiness checks. | It cannot record or replay arbitrary unmanaged terminal tabs. |
| Vision fallback | `greentic.desktop.vision` | Experimental | Screenshot/OCR/input backend configured with `GREENTIC_VISION_BACKEND_COMMAND` plus screen capture permission. | Sidecar readiness checks and visual fallback model. | Vision can help locate or inspect UI state, but it is not proof of durable side effects without a structured assertion or artifact check. |
| MCP stdio | `greentic-desktop mcp serve` | Beta | Local runtime initialized and runners saved in the runtime home. | MCP request/response schema and runner tool contract tests. | Use stdio for local assistant integrations where possible. |
| MCP HTTP | GUI-managed MCP service | Beta | GUI session token, localhost bind, saved ready runners. | Concurrent local HTTP service tests plus web runner MCP fixture execution with evidence persistence. | HTTP requests require the GUI session bearer token and must remain localhost-bound. |
| Evidence | Replay evidence bundle | Beta | Runtime evidence directory writable. | GUI/MCP replay writes `bundle.json`, `outputs.json`, and `trace.json` and returns an evidence reference. | Evidence can prove Greentic's observed run data; external side effects still need adapter-specific assertions. |
| LLM planning | `greentic-desktop-llm` through `greentic-llm` | Beta | Provider configured in settings and required API key saved in Greentic secrets. | Provider list, structured JSON prompt envelope, schema validation, and repair-loop tests. | Live provider behavior can still vary; invalid JSON must be repaired or surfaced as a schema error. |

Native desktop adapters are experimental until they have passing real fixture
tests for recording, replay, output extraction, assertions, and evidence on the
target operating system.
