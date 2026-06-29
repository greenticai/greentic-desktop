# PR-133 - Terminal Adapter Use portable-pty vte russh And ssh2

## Goal

Replace terminal/mainframe shelling and ad-hoc output handling with real PTY, terminal parsing, and SSH/TN3270 transport libraries.

## User Outcome

Terminal automation works against controlled sessions, can parse screen state reliably, and does not record unmanaged shell tabs.

## Current Evidence

- Terminal automation must own the session to be reliable.
- Screen output needs terminal emulation parsing, not raw string matching only.

## Scope

1. Add dependencies:
   - `portable-pty` for local controlled PTY sessions.
   - `vte` for terminal output parsing.
   - `russh` for pure-Rust SSH where feasible.
   - `ssh2` as an optional libssh2-backed alternative where needed.
2. Add controlled session model:
   - local PTY.
   - SSH session.
   - TN3270/mainframe session adapter boundary.
3. Implement actions:
   - send text.
   - send key.
   - wait for prompt/text/regex.
   - read screen buffer.
   - extract fields.
   - capture evidence.
4. Add terminal secrets handling:
   - credentials resolved outside runner YAML.
   - password prompts redacted in logs and evidence.
5. Make unmanaged terminal windows unsupported with concrete diagnostics.

## File Targets

- `crates/greentic-desktop-terminal/src/lib.rs`
- `crates/greentic-desktop-session/src/lib.rs`
- `crates/greentic-desktop-recorder/src/lib.rs`
- `docs/adapters/terminal-tn3270.md`
- `docs/recording-runbooks.md`

## Out of Scope

- Recording arbitrary Terminal/iTerm/Windows Terminal tabs.
- Embedding terminal credentials in runner files.

## Acceptance Tests

1. Local PTY fixture runs a command, parses output through `vte`, extracts a declared field, and returns it.
2. Password-like input is redacted in raw events, traces, and evidence.
3. SSH transport has a fake/server fixture or an ignored live test with explicit env requirements.
4. Unmanaged terminal recording fails closed with a clear reason.
5. MCP call and GUI run path use the same terminal replay implementation.

## Done Means

Terminal automation is session-owned and parser-backed instead of shell-output guesswork.
