# PR-92 - Real Playwright Web Replay and Recording

## Goal

Replace the in-memory `PlaywrightWebAdapter` replay path with real Playwright browser automation and real browser-context recording.

## User Outcome

For web workflows, Greentic opens a controlled browser, fills real forms, clicks real elements, waits for real page state, extracts real outputs, and records actual user interactions in that controlled browser.

## Current Evidence

- `PlaywrightWebAdapter::execute` mutates `WebState` and returns `"web step accepted"`.
- Web recording can start a Playwright recorder process, but replay does not drive Playwright.
- Existing browser tabs are not captured, which is acceptable if communicated clearly.

## Scope

1. Add a Playwright sidecar process for replay, not only recording.
2. Implement real actions:
   - `web.goto`
   - `web.fill`
   - `web.select`
   - `web.click`
   - `web.press`
   - `web.wait_for`
   - `web.wait_for_text`
   - `web.extract_text`
   - `web.extract_regex`
   - `web.assert_visible`
   - `web.screenshot`
   - `web.download_file`
3. Use semantic locators in priority order:
   - role/name
   - label
   - placeholder
   - test id
   - accessible name
   - CSS/XPath as fallback
4. Persist screenshots, DOM snapshots, console errors, and network failures in evidence.
5. Implement downloads with real filesystem verification.
6. Add web recording in Greentic-owned context via Playwright event listeners.
7. Remove in-memory visible text as product output source.
8. Add clear error if Node/Playwright/browser binaries are missing.

## E2E Fixtures

1. Local static form app served by test harness.
2. Local calculator-like web page for arithmetic output extraction.
3. Local file download page.

## Acceptance Tests

1. A web runner fills name/email in a real page and extracts the displayed confirmation id.
2. A web runner downloading a file only passes if the file exists on disk.
3. Recording in the Greentic-owned browser captures real click/type/navigation events.
4. Existing browser tabs are explicitly unavailable and produce a non-passing diagnostic.
5. Playwright replay failures include screenshot and DOM evidence.

