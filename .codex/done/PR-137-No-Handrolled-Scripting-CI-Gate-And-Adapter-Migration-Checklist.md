# PR-137 - No Handrolled Scripting CI Gate And Adapter Migration Checklist

## Goal

Prevent the repo from sliding back into per-adapter handrolled scripts once the foundation and library-backed adapters are introduced.

## User Outcome

New automation capabilities are built on approved libraries and shared abstractions, so behavior is consistent and production-readiness is measurable.

## Current Evidence

- Prior PRs repeatedly added local scripts/string protocols because there was no enforced architectural boundary.
- Without a CI gate, future fixes can reintroduce `osascript`, PowerShell UIA snippets, `curl`, `screencapture`, `wmctrl`, or fake protocol renderers.

## Scope

1. Add a CI script, for example `ci/no_handrolled_scripting_check.sh`.
2. Add a manifest dependency policy check:
   - dependency and dev-dependency versions live only in root `Cargo.toml`.
   - crate manifests may use only `*.workspace = true` dependency entries.
   - local crate-specific dependency versions, features, git refs, path overrides, and dev-dependency versions fail CI.
3. Maintain an allowlist for:
   - test fixtures.
   - docs examples.
   - foundation `SubprocessRunner`.
   - explicitly approved platform commands that have no library alternative.
4. Flag production code uses of:
   - `Command::new("curl")`
   - `Command::new("osascript")`
   - `Command::new("screencapture")`
   - `Command::new("wmctrl")`
   - `Command::new("xdotool")`
   - generated PowerShell UIA scripts.
   - manual HTTP/MCP string protocol construction after PR-128.
5. Add adapter migration checklist docs:
   - library used.
   - real fixture E2E.
   - permission preflight.
   - output proof.
   - secret redaction.
   - capability matrix status.
6. Wire the gate into `ci/local_check.sh` and GitHub Actions.

## File Targets

- `ci/no_handrolled_scripting_check.sh`
- `ci/local_check.sh`
- `.github/workflows/*`
- `docs/developer-notes.md`
- `docs/capability-matrix.md`

## Out of Scope

- Blocking all subprocess use.
- Removing subprocesses before replacement PRs land.

## Acceptance Tests

1. CI fails if production adapter code adds new direct `curl`, `osascript`, `screencapture`, `wmctrl`, `xdotool`, or generated PowerShell UIA usage outside the allowlist.
2. CI fails if manual MCP/HTTP protocol rendering is reintroduced after `rmcp`/`axum` migration.
3. The allowlist is small, documented, and reviewed.
4. Each adapter has a migration checklist entry with library, fixture, permission, proof, redaction, and maturity status.
5. `bash ci/local_check.sh` runs the new gate.
6. CI fails if a crate manifest adds a non-workspace dependency or dev-dependency declaration.

## Done Means

The codebase has an architectural guardrail that forces production adapters onto shared foundations and maintained libraries.
