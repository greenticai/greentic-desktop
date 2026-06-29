# PR-91 - Production Adapter Runtime and Sidecar Contract

## Goal

Define and implement the production contract between Greentic Desktop and real adapter backends.

## User Outcome

Installing an adapter means Greentic can start, health-check, execute, observe, and stop that adapter through a consistent protocol. The GUI can show whether each adapter is actually ready.

## Current Evidence

- Extension install state and adapter execution are separate concepts.
- Planner context currently sees capabilities from in-process model adapters.
- Recording backends are selected by ad hoc environment variables and fake blockers.

## Problem

Greentic needs a real runtime boundary. "Installed" must not mean "capability advertised" unless the adapter process is available, permitted, and healthy.

## Scope

1. Add `greentic-desktop-adapter-runtime` crate or module.
2. Define `AdapterRuntime` trait:
   - `manifest()`
   - `health()`
   - `preflight(session)`
   - `execute(step, inputs, secrets)`
   - `observe(target)`
   - `extract(output)`
   - `start_recording(session)`
   - `stop_recording(session)`
3. Define sidecar JSON-RPC or stdio protocol with strict serde schemas.
4. Add capability readiness statuses:
   - `installed`
   - `healthy`
   - `permission_blocked`
   - `sidecar_missing`
   - `unsupported_platform`
   - `not_implemented`
5. Change GUI planner context to include only `healthy` executable capabilities.
6. Add adapter health endpoint to GUI API and Settings UI.
7. Add adapter startup logs under runtime home, with per-adapter log files.
8. Make extension install/enable trigger adapter health refresh.

## Acceptance Tests

1. Installing an extension without an executable sidecar shows installed but not healthy.
2. A sidecar returning missing permission hides its executable capabilities from planner context.
3. GUI Settings reloads adapter readiness after install/remove without page refresh.
4. Runner execution refuses adapters whose health is not `healthy`.
5. MCP tool metadata includes adapter readiness diagnostics when a runner is unavailable.

## Migration Notes

Existing in-process Rust adapters can implement the runtime trait first. Later PRs can replace their bodies with OS-specific implementations or sidecars without changing GUI/MCP call paths.

