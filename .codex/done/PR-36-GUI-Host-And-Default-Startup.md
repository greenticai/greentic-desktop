# PR-36 - GUI Host and Default Browser Startup

## Goal

Make `greentic-desktop` with no arguments start the local GUI host, open the user's default browser, and display the embedded Automate Hub interface. CLI subcommands must keep working unchanged.

## User Outcome

On Windows, a user can download the `greentic-desktop.exe`, double-click it, and the default browser opens to the Greentic Automate Hub. From there they can create, record, validate, publish, and expose runners through follow-up PRs.

## Current State

- `greentic-desktop` without arguments currently returns CLI usage.
- `gtc desktop ...` requires the `desktop` prefix and is intentionally CLI-only.
- `DesktopRuntime::serve_mcp` binds an HTTP MCP shim but it is not a general GUI/static server.
- There is no browser opener or localhost GUI lifecycle.

## Scope

1. Add GUI startup mode to the installable `greentic-desktop` binary.
2. Keep `gtc desktop` as an explicit CLI surface.
3. Add an explicit CLI escape hatch:
   - `greentic-desktop gui`
   - `greentic-desktop gui --bind 127.0.0.1:0`
   - `greentic-desktop gui --no-open`
   - `greentic-desktop --help`
4. Preserve existing command behavior for all current subcommands.
5. Open the default browser to the local GUI URL on startup.
6. Keep the process alive while the GUI is in use.

## Behavior

### Default Invocation

```text
greentic-desktop
```

Expected behavior:

1. Initialize runtime home if needed.
2. Start GUI HTTP server on loopback.
3. Pick a free port by default.
4. Open the default browser to `http://127.0.0.1:<port>/`.
5. Print the URL to stdout for terminal users.
6. Keep serving until interrupted.

### CLI Invocation

Existing commands stay explicit:

```text
greentic-desktop info
greentic-desktop runner list
greentic-desktop record start ...
greentic-desktop mcp serve
```

### Windows Double-Click

For the `.exe` case:

- do not flash or depend on a terminal for core behavior
- open the default browser
- log startup failures to the runtime log path
- if browser opening fails, keep the server running and expose the URL through logs/stdout where possible

## Technical Plan

### New Runtime Module

Add a GUI host abstraction, likely in a new crate:

```text
crates/greentic-desktop-gui
```

Responsibilities:

- bind loopback server
- serve static frontend assets
- route `/api/*` requests to later API handlers
- route unknown non-API paths to `index.html`
- expose `GuiHost`, `GuiHostOptions`, `GuiHostHandle`

### Browser Opening

Use a small cross-platform browser open mechanism:

- macOS: `open <url>`
- Windows: `start`/ShellExecute equivalent
- Linux: `xdg-open <url>`

Prefer a well-maintained Rust crate if dependency policy allows it. If hand-rolled, keep it in one module and cover platform command selection with tests.

### Server Choice

The current code uses `std::net::TcpListener` for the scoped MCP shim. The GUI API will need route handling, JSON, static files, and request bodies. Pick a lightweight Rust HTTP approach and centralize it:

- option A: continue with `std::net` for minimal dependencies, but implement only simple routing
- option B: use a small HTTP server crate such as `tiny_http` or `axum` if async/API needs justify it

The PR should make an explicit decision and document it.

## Acceptance Criteria

- `greentic-desktop` with no args starts the GUI instead of printing usage.
- `greentic-desktop --help` still provides command help.
- `gtc desktop` without a subcommand still prints usage.
- Browser open is attempted by default and can be disabled with `--no-open`.
- GUI server serves `index.html`, static assets, and SPA fallback routes.
- Loopback binding is used by default.
- Unit tests cover CLI dispatch behavior.

## Test Plan

- `cargo test -p greentic-desktop`
- Start `greentic-desktop gui --no-open --bind 127.0.0.1:0` in an integration test or smoke test.
- Fetch `/`, `/create`, `/runners`, `/mcp`, `/settings`.
- Verify no-args dispatch starts GUI mode.
- Manual Windows smoke test: double-click `.exe`, browser opens.

## Risks

- A browser-based GUI means closing the browser tab does not automatically stop the process unless lifecycle logic is added later.
- Windows GUI subsystem changes may affect console output and CI behavior; keep binary subsystem decisions for packaging PRs.

