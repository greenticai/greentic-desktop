# Production Readiness Matrix

Greentic Desktop must either run against a real backend or block with a concrete reason. Installed means the extension manifest is present. Healthy means runtime health reports the sidecar, OS permission, or native provider needed for the capability.

Check live readiness from the GUI Settings page or:

```bash
curl http://127.0.0.1:<port>/api/v1/adapters/health
```

| Target | Replay readiness | Recording readiness | Notes |
| --- | --- | --- | --- |
| Web | Playwright sidecar available. | Greentic-owned browser context available. | Existing unrelated browser tabs are not recorded. |
| macOS desktop | Accessibility permission and macOS AX adapter available. | AX/event-tap event source and screen permission available. | When launched from Terminal, VS Code, Cursor, or iTerm2, grant permissions to that launcher or the debug binary. |
| Windows desktop | UIAutomation available on Windows. | UIA event source available. | Non-Windows hosts must report unavailable instead of passing. |
| Linux X11 | X11 tools and AT-SPI/window/input utilities available. | X11/AT-SPI event source command available. | Wayland global input remains restricted unless an explicit portal/provider is configured. |
| Java | Java Access Bridge sidecar command configured. | Java Access Bridge event source configured. | Word, Excel, and other native apps must not route to Java unless the target is a Java app. |
| Terminal/TN3270/SSH | `GREENTIC_TERMINAL_ADAPTER_COMMAND` points to an owned PTY/SSH/TN3270 runtime. | `GREENTIC_TERMINAL_RECORDER_COMMAND` points to an owned terminal event source. | Existing unmanaged terminal windows are not silently recorded. |
| Vision | `GREENTIC_VISION_BACKEND_COMMAND` points to screenshot/OCR/input backend. | Used by desktop/remote backends when screenshot evidence is required. | A missing OCR/input backend must hide executable vision capabilities. |
| Remote desktop | Owned viewport provider, screen capture, input control, and calibration are present. | `GREENTIC_REMOTE_VIEWPORT_PROVIDER_COMMAND` plus viewport calibration env vars are present. | Calibration uses `GREENTIC_REMOTE_VIEWPORT_X/Y/WIDTH/HEIGHT` and optional `GREENTIC_REMOTE_VIEWPORT_SCALE_PERCENT`. |

## Runner Migration

Older draft runners may contain adapter-id capability strings that no installed real backend can execute. The runner list marks these unavailable and includes the missing capability reason. Edit the runner via prompt or YAML so each step uses a capability exposed by `/api/v1/adapters/health`.

Declared outputs are required unless the runner schema explicitly marks them optional. If an output resolves to a local path, replay verifies that the file exists before reporting success.

## Release Gate

Run this before publishing or opening a release PR:

```bash
bash ci/no_mock_production_check.sh
bash ci/local_check.sh
```

The no-mock check fails if production paths reintroduce capability-only replay, fake recording backends, static adapter execution, or generic fake success messages.
