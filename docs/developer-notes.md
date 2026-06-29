# Developer Notes

This repository is a Rust workspace for Greentic Desktop. Most crates model a specific product feature area, and tests define expected behavior for the full desktop-runner system.

## Workspace Areas

- `greentic-desktop-core`: shared capability and runner-package policy types.
- `greentic-desktop-config`: default runtime configuration and TOML rendering.
- `greentic-desktop-runtime`: runtime host for config, discovery, extensions, telemetry, and MCP serving.
- `greentic-desktop-gui-assets`: embedded browser UI asset lookup for Automate Hub.
- `greentic-desktop-gui`: loopback HTTP host and browser opener for the Automate Hub GUI.
- `greentic-desktop`: installable package providing default GUI startup through `greentic-desktop` and explicit CLI commands through `greentic-desktop ...` or `gtc desktop ...`.
- `greentic-desktop-adapter`: common adapter capability, locator, step, and assertion models.
- `greentic-desktop-extension`: extension manifests, built-in adapters, sidecar metadata, and signed install checks.
- `greentic-desktop-recorder`: recording sessions, runner packages, portable steps, redaction, and YAML rendering.
- `greentic-desktop-planner`: prompt-to-runner draft planning.
- `greentic-desktop-refinement`: user correction parsing and scoped runner diffs.
- `greentic-desktop-replay`: replay validation and output traces.
- `greentic-desktop-evidence`: evidence records and references.
- `greentic-desktop-registry`: runner lifecycle, signatures, scopes, and promotion stages.
- `greentic-desktop-mcp`: publishing runners as MCP tools and calling them with policy checks.
- `greentic-desktop-security`: permissions, approvals, environments, signatures, secrets, and redaction.
- `greentic-desktop-ltm`: long-term memory for failures, corrections, root causes, and planner context.
- `greentic-desktop-workspaces`: AWS WorkSpaces runner exposure and forwarding model.
- `greentic-desktop-rollout`: canary patch validation and rollout decisions.
- `greentic-desktop-business`: business-process orchestration over desktop runners.
- `greentic-desktop-forwarded`: forwarded tool descriptors and call registration.
- `greentic-desktop-deployment`: connected and airgapped updates, dependencies, revocations, rollback, and audit logs.
- `greentic-desktop-mvp`: end-to-end modeled MVP flow.
- `greentic-desktop-platform`: platform detection, permission explanations, and support checks.
- `greentic-desktop-macos`, `greentic-desktop-linux`, `greentic-desktop-windowing`, `greentic-desktop-io`: platform-specific desktop models.
- `greentic-desktop-test-harness`: desktop test harness and sample target coverage metadata.
- `frontend/automate-hub`: React/TanStack source for the Automate Hub browser UI.

## Validation

Run the local validation script before publishing changes:

```bash
bash ci/local_check.sh
```

It runs formatting, Clippy, tests, builds, documentation generation, package checks, and publish dry-runs for publishable crates.

`ci/local_check.sh` also runs architectural guardrails:

- `ci/workspace_dependency_policy_check.sh` keeps dependency and dev-dependency versions in the workspace root only.
- `ci/no_mock_production_check.sh` blocks fake replay/recording success paths.
- `ci/no_handrolled_scripting_check.sh` blocks new per-adapter scripting surfaces such as `curl`, `osascript`, `screencapture`, `wmctrl`, `xdotool`, PowerShell UIA snippets, and manual MCP HTTP loops outside reviewed migration files.

## Adapter Migration Checklist

Before an adapter capability can move beyond experimental status, its PR must document:

- maintained library or shared foundation used for capture, input, protocol, or accessibility;
- real fixture E2E that proves input, side effect, output extraction, and evidence;
- permission preflight for the OS, session, or remote protocol;
- output proof, especially for files or durable business artifacts;
- secret redaction for traces, screenshots, terminal buffers, and evidence;
- capability matrix status and remaining unsupported paths.

Current migration boundaries:

| Adapter | Library-backed baseline | Still explicitly allowed while migrating |
| --- | --- | --- |
| Web | Playwright typed stdio protocol with request ids. | Node sidecar launch. |
| macOS | Shared `xcap` screenshot evidence. | Existing `osascript`/Swift AX migration file only. |
| Windows | Shared `xcap` screenshot evidence. | Existing PowerShell UIA migration file only. |
| Linux | Shared `xcap` screenshot evidence and Wayland fail-closed model. | Existing X11 `wmctrl`/`xdotool` migration file only. |
| Terminal | `portable-pty` local fixtures and `vte` parsing. | Configured owned runtime command boundary. |
| Java | Explicit Java target classifier; native apps route to OS accessibility. | Java Access Bridge sidecar command boundary. |
| Vision/Remote | Shared `xcap` screenshot path and explicit backend/provider requirements. | Configured OCR/input/remote viewport provider command boundary. |

Frontend validation is opt-in for local checks so Rust contributors do not need a JavaScript toolchain for unrelated changes:

```bash
GREENTIC_CHECK_FRONTEND=1 bash ci/local_check.sh
```

That mode installs frontend dependencies when needed and runs the Automate Hub build.

## Automate Hub Frontend

The browser UI source lives in `frontend/automate-hub`. It is built as a static bundle that later GUI host PRs serve from the Rust runtime.

Useful commands:

```bash
cd frontend/automate-hub
npm ci
npm run lint
npm run build
```

The generated `dist/` and `.output/` directories are ignored. The `greentic-desktop-gui-assets` crate embeds files from `frontend/automate-hub/dist` when that directory exists, and falls back to a small built-in placeholder when it does not. Release jobs must build the frontend before compiling `greentic-desktop` so the installed binary contains the full Automate Hub UI without requiring Node, npm, Bun, Vite, or source files on the user's machine.

## Release Notes

The installable `greentic-desktop` CLI package and the internal crates it depends on are publishable so `cargo binstall greentic-desktop` can resolve crate metadata and download release binaries.
