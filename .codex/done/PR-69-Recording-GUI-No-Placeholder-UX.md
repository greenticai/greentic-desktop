# PR-69 - Recording GUI With No Placeholder States

## Goal

Make the recording wizard accurately reflect real capture state and prevent users from believing unsupported recording is active.

## Problem

The UI currently shows recording-like screens even when no backend is capturing events. This creates false confidence and hides missing permissions/backends.

## Scope

1. Consume PR-63 capture state fields in the GUI.
2. Show target-specific setup before starting:
   - web: Greentic-owned browser recording
   - desktop: OS permissions and native backend
   - remote: screen/input permission and viewport calibration
   - Java: Java Access Bridge
   - terminal: Greentic-owned terminal/session profile
3. Disable Start Recording when required backend is unavailable.
4. Show "Capture backend active" only when heartbeat is fresh.
5. Show live event count, last event, screenshot count, and warnings.
6. Show blocked reasons prominently.
7. Remove copy that implies arbitrary external tabs/apps are recorded when they are not.
8. Add "Open controlled browser/terminal/app" affordances for owned-session backends.
9. Add recovery instructions when permissions change require app restart.

## UI States

Recording screen must distinguish:

- session created
- backend starting
- backend active
- paused
- observe-only
- blocked
- stopped
- failed

## Acceptance Criteria

- Starting a target without a backend shows blocked state, not an empty recording.
- Web recording tells the user Greentic opens a controlled browser.
- Terminal recording tells the user Greentic opens/connects a controlled terminal.
- Native/remote recording shows exact missing permissions.
- Event count increments when fake/live backend emits events.
- Stop button is disabled or explained when no backend exists.
- Normalise is disabled until there are real captured events or explicit manual markers.

## Test Plan

- Frontend tests with mocked active backend.
- Frontend tests with mocked blocked backend.
- Playwright test for web recording fixture once PR-64 lands.
- Playwright test for no-backend target showing blocked state.

## Done Means

The UI no longer has any recording placeholder path. Every target is either actively captured, observe-only with explanation, or blocked.

