# PR-66 - Java App Recording Through Java Accessibility

## Goal

Make Java Swing/AWT recording real by using Java Accessibility events and component trees, not generic screenshots or placeholder session state.

## Problem

Java apps often expose richer component metadata through Java Accessibility than native OS APIs. The existing Java adapter can model generic workflows, but recording does not subscribe to Java accessibility events or produce Java-specific locators.

## Scope

1. Add a Java recording backend plugged into PR-63.
2. Detect and validate Java Access Bridge availability.
3. Subscribe to Java accessibility events:
   - focus change
   - action/click
   - text changed
   - value changed
   - selection changed
   - window opened/closed
4. Capture component path and stable locator candidates:
   - accessible name
   - role
   - label relation
   - component class
   - index path fallback
5. Capture component tree snapshots around events.
6. Normalize Java events to semantic workflow actions:
   - `java.find_window`
   - `java.find_component`
   - `java.click`
   - `java.type_text`
   - `java.select`
   - `java.read_text`
   - `java.assert_text`
7. Redact password fields and secret-looking values.

## Fixture App

Add a small Java Swing fixture app with:

- two text fields
- operation combo box/buttons
- calculate button
- result label
- password field for redaction test

## Acceptance Criteria

- Recording the Java calculator fixture produces input fields `number_1`, `number_2`, `operation` and output `result`.
- Replaying the recorded Java runner against the fixture returns `2` for `1 + 1`.
- Password field events are redacted and become secret candidates.
- Missing Java Access Bridge blocks recording with setup instructions.
- Component locators use accessible metadata before index fallback.

## Test Plan

- Unit tests for Java component locator generation.
- Fixture E2E test on supported OS/JDK where Java Access Bridge is available.
- Redaction test for `JPasswordField`.
- Normalization test from captured Java event JSONL to runner YAML.

## Done Means

"Java app task" recording captures real Java UI events and produces replayable Java runners.

