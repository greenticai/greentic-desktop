# PR-82 - Real Replay Service for GUI and MCP

## Goal

Create a production replay service that loads typed runner manifests, resolves inputs/secrets, dispatches each step to installed adapters, executes assertions, extracts outputs, persists evidence, and returns the same result shape to GUI and MCP callers.

## User Outcome

Pressing Run in the GUI and invoking the MCP tool do the same real automation. The user sees the target app/browser/terminal being driven when the adapter is local and visible, unless the selected adapter is explicitly headless or remote.

## Current Evidence

- `greentic-desktop-replay` can dispatch through a `DesktopAdapter` registry, but GUI/MCP does not use it.
- GUI/MCP run paths manufacture passed responses.
- Adapter registry construction in GUI is not wired to real installed adapter instances.

## Problem

Without a real replay service in the GUI host, the application cannot perform user-described desktop automation. Existing replay code is isolated and often uses `CapabilityOnlyAdapter`, which makes tests pass without real side effects.

## Scope

1. Add `GuiReplayService`.
2. Build an adapter registry from installed/enabled extensions and platform.
3. Load secrets from `greentic-secrets-lib`.
4. Resolve input values using typed schemas.
5. Dispatch compiled steps to real adapter implementations.
6. Execute assertions and output extractors.
7. Persist evidence bundles and expose them in the UI.
8. Return structured errors that surface in red GUI banners.
9. Use the service for all GUI and MCP run/test paths.

## Required Error Codes

- `runner.input_missing`
- `runner.secret_missing`
- `runner.adapter_unavailable`
- `runner.capability_missing`
- `runner.permission_missing`
- `runner.step_failed`
- `runner.output_extraction_failed`
- `runner.assertion_failed`

## Acceptance Tests

1. GUI Run and MCP Call both invoke the same replay service.
2. Missing input fields block before adapter execution.
3. Missing adapter produces setup guidance.
4. A fixture adapter that records executed steps receives every step in order.
5. Output extractor values come from adapter observations, not hard-coded output names.
6. Evidence is persisted for success and failure.

