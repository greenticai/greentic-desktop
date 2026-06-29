# PR-124 - Evidence Telemetry Persistence Proof

## Goal

Make evidence and telemetry real persisted artifacts rather than mostly model assertions.

## User Outcome

Every runner execution returns evidence refs that resolve to actual files containing useful run data, screenshots/logs where available, tool traces, outputs, and failure diagnostics.

## Current Evidence

- Evidence models exist, but audit questions whether refs resolve to real artifacts.
- Telemetry is tiny and mostly in-memory/model-level.

## Scope

1. Evidence store contract:
   - bundle path layout.
   - immutable run id.
   - artifact metadata.
   - output extraction proof.
2. Persist real artifacts:
   - MCP transcript for E2E.
   - Playwright screenshot/trace for web slice.
   - replay step trace.
   - failure diagnostics.
3. Evidence lookup API:
   - GUI can open/download evidence bundle.
   - CLI can inspect evidence ref.
4. Telemetry:
   - persisted local event log.
   - redaction by default.
   - no secrets.
5. Tests:
   - returned evidence ref resolves to bundle on disk.
   - referenced artifact files exist.
   - secret values are redacted.

## File Targets

- `crates/greentic-desktop-evidence/src/lib.rs`
- `crates/greentic-desktop-telemetry/src/lib.rs`
- `crates/greentic-desktop-replay/src/lib.rs`
- `crates/greentic-desktop-gui/src/lib.rs`
- `crates/greentic-desktop-mcp/src/lib.rs`

## Out of Scope

- Remote evidence upload.
- Long-term retention policy UI.

## Acceptance Tests

1. E2E web run returns an evidence ref.
2. Evidence ref resolves to a bundle JSON file.
3. Every artifact referenced by bundle exists.
4. Failed run contains failed step id and reason.
5. Secret input does not appear in evidence or telemetry files.

## Done Means

Evidence refs are useful operational artifacts, not decorative strings.
