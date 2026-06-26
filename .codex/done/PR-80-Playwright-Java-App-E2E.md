# PR-80 - Playwright Java App E2E

## Goal

Add end-to-end coverage for Java desktop automation using a deterministic Swing fixture app and the Java accessibility adapter path.

## Problem

Java apps have a distinct automation surface from native desktop apps. The system should prove it can open a Java app, identify Swing/AWT controls, provide inputs, click actions, read outputs, record/replay, and report clear setup errors when the Java accessibility bridge is unavailable.

## Fixture App

Add a small Java Swing fixture under `fixtures/java/customer-form`:

- window title: `Greentic Java Fixture`
- text field: `customer_name`
- text field: `email`
- button: `Save`
- label/output: `confirmation_id`
- deterministic output: `CONF-<stable hash>`

The fixture must expose accessible names/descriptions for controls.

## Scope

1. Add fixture build script:
   - compile with installed JDK
   - skip gracefully if JDK missing
2. Add `e2e/java-app.spec.ts`.
3. Add setup/config test:
   - Java adapter appears in extension search
   - install Java adapter
   - health reports ready or exact missing dependency
4. Add prompt runner flow:
   - prompt: `Open the Java customer form, enter customer_name and email, click Save, and return confirmation_id`
   - generated runner uses Java capabilities
   - input fields are present
   - run with sample values
   - output matches fixture
5. Add recording flow:
   - choose Desktop app or Java app target if added
   - start fixture app
   - capture fake/deterministic Java accessibility events in CI
   - normalise/save/test
6. Add optional real Java accessibility test:
   - requires `GREENTIC_DESKTOP_REAL_JAVA=1`
   - starts Swing app
   - uses actual accessibility bridge where supported
   - skips with setup instructions if unavailable

## Acceptance Criteria

- Java adapter appears in GUI extension flow.
- Prompt-generated Java runner has inputs and outputs.
- CI can run the fake Java app path without real desktop permissions.
- Optional real Java fixture can run locally on a prepared machine.
- Missing JDK or bridge produces a clear skipped/manual diagnostic, not a failure hidden as "runner failed".

## Test Plan

```bash
npm --prefix frontend/automate-hub run e2e -- --grep "@java"
cargo test -p greentic-desktop-java -p greentic-desktop-test-harness java_app_workflow_e2e_installs_extension_and_returns_output
```

## Risks

- Java accessibility differs by OS and JDK. Keep mandatory CI on fake/contract mode and real tests explicit.
