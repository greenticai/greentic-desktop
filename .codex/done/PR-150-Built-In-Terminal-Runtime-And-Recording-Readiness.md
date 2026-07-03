# PR-150 - Built-In Terminal Runtime and Recording Readiness

## Goal

Remove unnecessary `sidecar_missing` status for basic terminal automation by making the built-in local terminal command runner and owned PTY runtime first-class production capabilities.

## User Outcome

Users can run and record local terminal automations without configuring `GREENTIC_TERMINAL_ADAPTER_COMMAND`. Advanced SSH/TN3270 backends can still be optional, but the common local terminal path works out of the box.

## Current Evidence

- `TerminalAdapter::execute` has a real `terminal.run_command` path.
- Owned PTY tests exist for local command execution and recording.
- Health still reports `Owned terminal PTY/SSH/TN3270 runtime is not configured. Set GREENTIC_TERMINAL_ADAPTER_COMMAND.`
- Recording target text says terminal/mainframe requires an owned runtime, but local owned shell support is already partly present.

## Scope

1. Promote local command and local owned PTY support to a built-in terminal backend.
2. Split terminal capabilities:
   - built-in local shell: `terminal.run_command`, local output extraction, basic recording of owned sessions.
   - configured session runtime: `terminal.connect`, `terminal.disconnect`, `terminal.read_screen`, `terminal.wait_for_screen`, SSH/TN3270 actions.
3. Add runtime config persisted in Greentic config:
   - default shell command
   - default working directory
   - timeout
   - optional SSH/TN3270 backend command
4. Update recording start for terminal:
   - local command/session recording starts without env var.
   - unmanaged external terminals remain blocked with clear message.
5. Add Windows-safe bounded command execution tests to prevent CI hangs.
6. Update adapter health and runner validation to reflect partial terminal readiness.

## Acceptance Tests

1. Fresh install shows terminal adapter healthy for `terminal.run_command`.
2. Terminal top-10-largest-files runner executes without `GREENTIC_TERMINAL_ADAPTER_COMMAND`.
3. Local terminal recording captures command output from an owned session.
4. SSH/TN3270 capabilities remain blocked until configured.
5. Windows terminal tests finish under 10 seconds and cannot hang indefinitely.

