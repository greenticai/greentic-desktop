# Developer Notes

This repository is a Rust workspace for Greentic Desktop. Most crates model a specific product feature area, and tests define expected behavior for the full desktop-runner system.

## Workspace Areas

- `greentic-desktop-core`: shared capability and runner-package policy types.
- `greentic-desktop-config`: default runtime configuration and TOML rendering.
- `greentic-desktop-runtime`: runtime host for config, discovery, extensions, telemetry, and MCP serving.
- `greentic-desktop`: installable CLI package providing `greentic-desktop` and `gtc desktop` command entry points.
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

## Validation

Run the local validation script before publishing changes:

```bash
bash ci/local_check.sh
```

It runs formatting, Clippy, tests, builds, documentation generation, package checks, and publish dry-runs for publishable crates.

## Release Notes

The installable `greentic-desktop` CLI package and the internal crates it depends on are publishable so `cargo binstall greentic-desktop` can resolve crate metadata and download release binaries.
