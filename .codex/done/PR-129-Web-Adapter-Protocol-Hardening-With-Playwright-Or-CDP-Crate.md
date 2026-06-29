# PR-129 - Web Adapter Protocol Hardening With Playwright Or CDP Crate

## Goal

Keep Playwright as the primary web automation engine, but replace fragile subprocess-sidecar behavior with a typed protocol and evaluate Rust-native alternatives where they reduce operational risk.

## User Outcome

Web recording and replay remain best-in-class while sidecar failures become diagnosable, typed, and non-fragile.

## Current Evidence

- Playwright is the right automation engine for web, but the sidecar protocol is still custom and easy to break.
- Subprocess lifecycle, stderr parsing, and event transport need a stable JSON-RPC contract.
- Rust-native CDP/WebDriver crates may be useful for focused use cases but should not regress Playwright coverage.

## Scope

1. Define a typed stdio JSON-RPC protocol between Greentic and the Playwright sidecar.
2. Add request/response schemas for:
   - launch context.
   - navigate.
   - fill/click/select.
   - record start/stop.
   - observe/extract.
   - screenshot/download evidence.
3. Use the foundation subprocess runner for sidecar launch and diagnostics.
4. Add timeout, heartbeat, and sidecar crash recovery semantics.
5. Evaluate `chromiumoxide`, `thirtyfour`, and `fantoccini` in a short architecture note:
   - keep Playwright for recording and cross-browser flows unless the note proves otherwise.
   - optionally add a `chromiumoxide` CDP backend only for Chromium-only environments.

## File Targets

- `crates/greentic-desktop-web/src/lib.rs`
- `crates/greentic-desktop-web/sidecar/*`
- `crates/greentic-desktop-adapter/src/lib.rs`
- `docs/adapters/playwright-web.md`
- `docs/capability-matrix.md`

## Out of Scope

- Removing Playwright without equivalent recording coverage.
- Browser extension redesign.

## Acceptance Tests

1. Web replay uses typed sidecar request/response structs, not ad-hoc stdout parsing.
2. Web recording events arrive over a typed stream with sequence ids.
3. Sidecar crash produces a structured adapter error with redacted diagnostics.
4. The existing web MCP vertical-slice E2E still passes.
5. Architecture note documents whether `chromiumoxide`, `thirtyfour`, or `fantoccini` will be adopted.

## Done Means

The web adapter keeps Playwright’s strengths while removing brittle process/protocol glue.
