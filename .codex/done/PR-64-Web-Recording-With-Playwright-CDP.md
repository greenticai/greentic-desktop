# PR-64 - Web Recording With Playwright/CDP

## Goal

Make browser recording real. When a user chooses "Browser task", Greentic must open or attach to a browser page, capture navigation/click/type/select/submit events, generate stable locators, and turn the recording into a replayable web runner.

## Problem

The current browser recording flow does not attach to the browser tab. If the user goes to another tab and performs a workflow, no DOM events, navigation, inputs, screenshots, or selectors are captured.

## Required Design

Use Playwright/CDP as the first real web recorder because it is reliable, cross-platform, testable in CI, and avoids requiring a browser extension for the initial implementation.

## Scope

1. Add a `greentic-desktop-web-recorder` crate or module.
2. Start a Playwright recorder sidecar when a browser recording session starts.
3. Open a controlled browser context/page from Greentic.
4. Inject lightweight event listeners for:
   - click
   - input/change
   - key press where semantically meaningful
   - submit
   - navigation
   - download/upload prompts
   - dialog open/accept/dismiss
5. Capture locator candidates for each event:
   - role/name
   - label
   - placeholder
   - test id
   - accessible name
   - CSS fallback
   - XPath fallback only as last resort
6. Capture DOM snapshots and screenshots at useful checkpoints.
7. Redact password/token fields before writing raw events.
8. Emit `recording.event.v1` events through PR-63's event sink.
9. Normalize web events into `DesktopWorkflow`/runner actions:
   - `web.goto`
   - `web.click`
   - `web.fill`
   - `web.select`
   - `web.press`
   - `web.wait_for`
   - `web.extract_text`
10. Add tests with a local fixture web app.

## Browser Ownership Rules

Initial implementation should support a Greentic-owned browser context only.

- The UI must say "Greentic will open a browser window for recording."
- Recording arbitrary existing tabs is explicitly not supported until a browser extension or persistent CDP attach flow exists.
- If the user switches to an external tab, the GUI should say it is outside the recorded context.

## Fixture App

Add a local fixture page for E2E tests:

- calculator form
- login-like form with secret field
- table/search result
- multi-step wizard
- downloadable result

## Acceptance Criteria

- Browser recording opens a browser window controlled by Greentic.
- User actions on the controlled page append real events to `events.jsonl`.
- Normalization produces a runner with web actions and stable locators.
- Secret fields are redacted in raw events and runner YAML.
- Replaying the recorded runner against the fixture app succeeds.
- The UI shows live event count while recording.
- The UI does not claim it can record arbitrary existing browser tabs.

## Test Plan

- Playwright E2E test records the calculator fixture:
  - enter `1`
  - choose `+`
  - enter `1`
  - submit
  - mark/read `result`
  - normalise
  - replay
  - assert output `2`
- Playwright test records a login fixture and asserts the password is redacted.
- Backend integration test verifies raw event schema.
- Snapshot test verifies locator candidates include role/name before CSS fallback.

## Done Means

"Browser task" recording is no longer a placeholder. It either records the Greentic-owned browser page or clearly blocks with a reason.

