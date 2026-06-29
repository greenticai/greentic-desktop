# PR-125 - Documentation Truthfulness and Capability Matrix

## Goal

Align documentation and UI claims with what actually works today, and separate production-ready paths from experimental/model-only paths.

## User Outcome

Users know which workflows are supported, which require permissions or sidecars, and which are experimental. The docs stop implying production readiness where the runtime is not proven.

## Current Evidence

- Audit says docs overstate maturity and hide that many paths are model-only.
- The user repeatedly expected native desktop automation to work end-to-end based on UI/docs language.

## Scope

1. Add capability matrix:
   - web Playwright.
   - macOS AX.
   - Windows UIA.
   - Linux X11/Wayland.
   - Java.
   - Terminal.
   - Vision.
   - MCP stdio/HTTP.
2. For each capability list:
   - status: production / beta / experimental / model-only.
   - required extensions.
   - required OS permissions.
   - known limitations.
   - tested fixture coverage.
3. Update README:
   - emphasize the one proven vertical slice.
   - remove production claims for unproven adapters.
4. Update GUI:
   - badges in Extensions/Setup/Runner test explaining capability status.
   - warnings before running experimental adapters.
5. Add a docs test:
   - generated capability matrix from manifests/tests to avoid drift.

## File Targets

- `README.md`
- `docs/*`
- `frontend/automate-hub/*`
- `crates/greentic-desktop-gui/src/lib.rs`
- `crates/greentic-desktop-extension/src/lib.rs`

## Out of Scope

- Marketing copy.
- Website redesign.

## Acceptance Tests

1. Docs mention MCP stdio as the supported path after PR-115.
2. Docs mark native desktop adapters experimental until their fixture E2Es pass.
3. GUI setup page shows permission and maturity status.
4. Capability matrix generation test fails when an adapter manifest lacks status.

## Done Means

No user should infer a path is production-ready unless an E2E proves it.
