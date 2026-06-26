# PR-68 - Recorder End-to-End Fixtures and CI Harness

## Goal

Add repeatable tests proving that recording creates real reusable runners across web, terminal, and representative desktop targets.

## Problem

The current tests prove lifecycle files are created and finalised, but they do not prove that human activity is captured, normalized, replayed, and exposed as a runner/MCP tool.

## User Outcome

Before release, CI and local checks can prove:

- recording captures real events where the environment supports it
- generated runners can replay
- outputs are extracted
- secrets are redacted
- unsupported OS/session states are reported correctly

## Test Matrix

### Always-on CI

Use deterministic fake and structured backends:

- fake native recorder emits accessibility events
- web recorder uses local Playwright test page
- terminal recorder uses a local command/PTY fixture
- replay verifies generated outputs
- MCP tool invokes generated runner

### OS-Specific CI

macOS:

- unit tests for AX/CoreGraphics event conversion with fake events
- optional/manual ignored test for real permissions

Windows:

- unit tests for UI Automation event conversion with fake COM event records
- optional/manual ignored test for real Calculator/Notepad recording

Linux:

- X11 test under `xvfb-run` where possible
- AT-SPI fake/session tests
- Wayland blocked-state tests without requiring compositor-specific CI

## Fixtures

Add fixtures under `crates/greentic-desktop-test-harness`:

- `web_form_fixture`
- `terminal_sum_fixture`
- `native_calculator_event_fixture`
- `native_text_entry_event_fixture`
- `remote_visual_event_fixture`

Each fixture should assert the same contract:

```text
record -> normalize -> review warnings -> run -> extract outputs -> publish MCP -> call MCP
```

## Local Check Integration

Extend `ci/local_check.sh` with fast recorder checks:

- recorder runtime unit tests
- normalization golden tests
- GUI API fake backend test
- web fixture if Playwright/browser dependencies are present
- terminal fixture

Do not make native permissioned tests mandatory in local check. Provide explicit commands:

```bash
cargo test -p greentic-desktop-test-harness --features native-recording-manual -- --ignored
```

## Acceptance Criteria

- A fake event stream can produce a valid runner and MCP tool in CI.
- Web recording fixture runs in CI and extracts an output.
- Terminal recording fixture runs in CI and extracts an output.
- Native OS conversion tests run without requiring real desktop permissions.
- Manual native tests are documented and skipped unless explicitly requested.
- `local_check.sh` catches regressions in recorder runtime, normalization, replay, and MCP exposure.

## Test Plan

This PR is the test plan for PR-63 through PR-67. It should land after the first fake backend and before relying on native manual testing.

## Risks

- Full native GUI automation in hosted CI is brittle. Keep CI focused on conversion contracts and use manual ignored tests for permissioned OS behavior.
- Playwright browser installs can slow CI. Cache browsers or use the system browser where available.

