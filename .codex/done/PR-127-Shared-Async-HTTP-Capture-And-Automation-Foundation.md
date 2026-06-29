# PR-127 - Shared Async HTTP Capture And Automation Foundation

## Goal

Create one production foundation crate that all adapters use for async execution, HTTP, input capture, input synthesis, screenshots, command rendering, and secret-safe diagnostics.

## User Outcome

Adapters stop each inventing their own blocking loops, subprocess wrappers, screenshot commands, and input injection paths. New adapter work starts from reliable shared primitives.

## Current Evidence

- Multiple adapters still use direct `std::process::Command` calls for OS scripts, sidecars, screenshots, or shell wrappers.
- GUI/MCP has a hand-rolled HTTP server and request parser.
- Recording and replay paths do not share one capture/synthesis abstraction.
- Secret-safe command rendering exists but is not enforced as the only subprocess diagnostic path.

## Scope

1. Add a shared foundation crate, for example `greentic-desktop-automation-foundation`.
2. Introduce these dependencies centrally:
   - `tokio` for async runtime.
   - `reqwest` for HTTP clients.
   - `rdev` for keyboard/mouse capture.
   - `enigo` for keyboard/mouse synthesis baseline.
   - `xcap` for screenshots and monitor/window capture.
   - `schemars` and `jsonschema` for schema generation and validation.
   - `ed25519-dalek` and `sha2` for signatures and digests.
3. Declare every new dependency and dev-dependency version only in root `Cargo.toml` under `[workspace.dependencies]`.
4. Crate manifests may only opt in with `dependency.workspace = true`; they must not set local dependency versions, features, path overrides, or dev-dependency versions.
5. Add foundation traits:
   - `EventCaptureBackend`.
   - `InputSynthesisBackend`.
   - `ScreenshotBackend`.
   - `HttpClient`.
   - `SubprocessRunner` with redacted command display.
   - `JsonSchemaValidator`.
6. Add platform capability detection:
   - capture available/unavailable with concrete reason.
   - synthesis available/unavailable with concrete reason.
   - screenshot available/unavailable with concrete reason.
7. Move all new subprocess diagnostics through the shared redaction API.

## File Targets

- `Cargo.toml`
- `crates/greentic-desktop-automation-foundation/*`
- `crates/greentic-desktop-adapter/src/lib.rs`
- `crates/greentic-desktop-security/src/lib.rs`
- `crates/greentic-desktop-platform/src/lib.rs`

## Out of Scope

- Per-OS accessibility tree traversal.
- Rewriting every adapter in this PR.
- GUI redesign.

## Acceptance Tests

1. Foundation exposes async HTTP, screenshot, capture, and synthesis traits behind stable Rust interfaces.
2. Tests prove `SubprocessRunner` redacts bearer tokens, API keys, passwords, and known secret values from rendered diagnostics.
3. Tests prove `xcap` screenshot backend returns a real file path or a concrete unsupported/permission error.
4. Tests prove `rdev` capture backend can be marked unavailable without falling back to fake events.
5. No adapter crate directly introduces a new secret-bearing subprocess wrapper outside the foundation.
6. A manifest lint test fails if any crate `Cargo.toml` declares dependency or dev-dependency versions outside the root workspace manifest.

## Done Means

All future adapter work has a common production spine and no longer needs to hand-roll async, HTTP, capture, screenshot, schema, or redacted subprocess behavior.
