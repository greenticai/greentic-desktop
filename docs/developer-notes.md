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
