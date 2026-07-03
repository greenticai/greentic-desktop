# PR-149 - Adapter Readiness Setup and Capability Probes

## Goal

Make the Settings > Adapter readiness panel actionable and accurate. Installed extensions should not show generic `not_implemented` or `sidecar_missing` messages unless there is genuinely no executable backend, and every missing backend should expose a concrete setup action.

## User Outcome

When a user sees Java, terminal, or vision as unavailable, Greentic explains exactly what is missing, offers a setup/fix action where possible, refreshes readiness after setup, and only advertises executable capabilities that can run on the current machine.

## Current Evidence

- Java currently appears as `not_implemented` even though the adapter code supports an external Java Access Bridge command through `GREENTIC_JAVA_ACCESS_BRIDGE_COMMAND`.
- Terminal currently appears as `sidecar_missing` when `GREENTIC_TERMINAL_ADAPTER_COMMAND` is absent, even though `terminal.run_command` has a real built-in local command path.
- Vision correctly needs a backend command, but the UI only reports an env var and does not provide install/setup guidance.
- Adapter health is mostly derived from static capability contracts plus env var checks; it does not expose per-capability readiness or setup actions.

## Scope

1. Introduce an `AdapterReadinessProbe` model with:
   - adapter id
   - current platform
   - executable capabilities
   - blocked capabilities
   - missing setup items
   - setup actions
   - log path
2. Replace binary adapter-level health with per-capability readiness:
   - `terminal.run_command` can be healthy without a TN3270 sidecar.
   - `terminal.connect`, `terminal.read_screen`, and TN3270/SSH session actions require an owned terminal runtime.
   - Java actions require Java Access Bridge command availability.
   - Vision actions require screenshot, OCR, and input backend availability.
3. Add `/api/v1/adapters/:id/setup` or equivalent action endpoint.
4. Make setup actions idempotent and refresh the readiness panel after they run.
5. Write adapter probe logs with the exact commands/env/config checked.
6. Update runner validation to use per-capability readiness instead of whole-adapter status.

## Acceptance Tests

1. Terminal without `GREENTIC_TERMINAL_ADAPTER_COMMAND` reports `terminal.run_command` healthy and TN3270/SSH capabilities blocked.
2. Java without `GREENTIC_JAVA_ACCESS_BRIDGE_COMMAND` reports `sidecar_missing`, not `not_implemented`, with a setup action.
3. Vision without backend reports separate screenshot/OCR/input missing setup details.
4. The GUI readiness endpoint returns setup actions and blocked capability details.
5. Clicking setup/fix refreshes the readiness panel without requiring a manual page reload.

