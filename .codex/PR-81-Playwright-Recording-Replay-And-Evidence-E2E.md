# PR-81 - Playwright Recording, Replay, and Evidence E2E

## Goal

Verify that recording workflows create real reusable runners with evidence, assertions, output extraction, and replay behavior visible through the GUI.

## Problem

The recording UI can appear to work while only creating lifecycle markers. Users need confidence that a recording captured real events, normalized them into meaningful steps, tested correctly, and saved a runner that can be run again.

## Scope

1. Add `e2e/recording-replay.spec.ts`.
2. Use deterministic fake recorder backends from PR-75.
3. Cover recording targets:
   - browser fixture
   - native calculator fake fixture
   - terminal fixture
   - remote/vision fake fixture
4. For each target, test:
   - create recording session
   - capture status is active or blocked with reason
   - event count increases with real adapter events
   - screenshots/evidence count appears when applicable
   - stop recording
   - normalise
   - warnings are shown if capture was inactive
   - test recorded runner
   - finalise/save
   - runner appears on Runners page
   - run saved runner with inputs
   - output matches expected value
5. Evidence checks:
   - evidence bundle appears after run/test
   - artifact link opens
   - screenshots/tool trace are redacted where needed
   - raw secret values are absent
6. Crash/recovery check:
   - start recording
   - kill GUI process
   - restart with same runtime home
   - session appears as failed/recoverable with evidence intact

## Fixtures

Add deterministic fixture scenarios:

- web: invoice lookup returns `42.50`
- native fake: calculator `1 + 1 = 2`
- terminal: account balance `100.00`
- vision: OCR text `Approved`

## Acceptance Criteria

- Recording UI distinguishes real captured events from lifecycle markers.
- Normalized runner steps use adapter semantics, not generic placeholder actions.
- Saved runner can be replayed from the Runner page.
- Evidence is visible and redacted.
- Blocked capture is clearly explained and does not pretend success.
- Restart recovery preserves session metadata.

## Test Plan

```bash
npm --prefix frontend/automate-hub run e2e -- --grep "@recording|@replay"
cargo test -p greentic-desktop-recorder -p greentic-desktop-replay -p greentic-desktop-test-harness recorder_fixture_record_normalize_returns_output
```

## Risks

- Full visual evidence screenshots can be large. Keep CI artifacts compact and attach full screenshots only on failure.
