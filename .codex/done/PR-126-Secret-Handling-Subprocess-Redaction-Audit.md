# PR-126 - Secret Handling Subprocess Redaction Audit

## Goal

Ensure secrets never appear in subprocess arguments, evidence, telemetry, logs, or GUI errors.

## User Outcome

Users can configure LLM/API/application credentials without leaking them through process listings or artifacts.

## Current Evidence

- LLM currently shells out to `curl`, which can expose API keys in process arguments.
- Evidence and logs need proof that redaction works across all failure paths.

## Scope

1. Inventory all subprocess calls:
   - `curl`
   - `osascript`
   - `playwright`
   - `screencapture`
   - Swift/event tap sidecars.
2. Replace secret-bearing subprocess args:
   - with HTTP client from PR-120.
   - or stdin/env only where safe and documented.
3. Add central redaction API:
   - redacts known secret values.
   - redacts common token/key patterns.
   - applies to errors, evidence, telemetry, MCP responses.
4. Add tests:
   - fake secret passed through failed LLM request does not appear in error.
   - fake secret passed as runner input does not appear in evidence.
   - subprocess command renderer never includes secret values.
5. Add security CI check:
   - grep-like guard for known test secret values in generated evidence/log fixtures.

## File Targets

- `crates/greentic-desktop-security/src/lib.rs`
- `crates/greentic-desktop-llm/src/lib.rs`
- `crates/greentic-desktop-replay/src/lib.rs`
- `crates/greentic-desktop-evidence/src/lib.rs`
- `crates/greentic-desktop-gui/src/lib.rs`
- `ci/local_check.sh`

## Out of Scope

- Enterprise KMS integration.
- Cloud secret sync.

## Acceptance Tests

1. `DEEPSEEK_API_KEY` value is never in process args because no curl subprocess is used.
2. Failed runner with secret input redacts the value in GUI/MCP error.
3. Evidence bundle contains secret refs or redacted markers, not raw values.
4. Telemetry event file contains no raw secret.
5. Local security check fails if a known fake secret appears in generated artifacts.

## Done Means

Secrets are treated as operationally sensitive data across the full runtime, not just schema fields.
