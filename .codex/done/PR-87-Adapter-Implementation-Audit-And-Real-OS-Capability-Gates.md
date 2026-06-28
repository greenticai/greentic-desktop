# PR-87 - Adapter Implementation Audit and Real OS Capability Gates

## Goal

Audit every adapter crate and ensure advertised capabilities correspond to real implemented behavior or are gated behind explicit unsupported/preflight diagnostics.

## User Outcome

Greentic will not claim it can automate a desktop app unless the installed adapter can actually perform the required operation on the current OS/session.

## Current Evidence

- Some adapters seed in-memory state for tests.
- Recorder backends may emit synthetic focused-app events.
- Planner context advertises static capabilities without checking installed extensions or permissions.

## Problem

Capability overstatement creates false confidence. A generic automation product needs a strict contract: if a capability is advertised to the planner, replay must be able to execute it or preflight must block.

## Scope

1. For each adapter, classify capabilities:
   - implemented
   - observe-only
   - planned/unsupported
   - test fixture only
2. Add preflight checks for permissions and session type.
3. Remove unsupported capabilities from planner context.
4. Make unsupported executions return typed adapter errors.
5. Add adapter contract tests:
   - advertised capability executes in a fixture or returns preflight blocked
   - unsupported capabilities are never advertised
6. Update setup UI to show adapter-level readiness.

## Acceptance Tests

1. macOS AX only advertises automation when Accessibility/Input permissions are available.
2. Wayland restricted input is not advertised as full desktop automation.
3. Web adapter requires Playwright/browser sidecar for recording.
4. Terminal adapter requires configured PTY/session source for recording.
5. Planner cannot select an adapter that is installed but not ready.

