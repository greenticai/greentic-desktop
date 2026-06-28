# PR-70 - Recording Normalization to Generic Workflows

## Goal

Convert raw recorded events from every backend into generic, replayable `DesktopWorkflow` and runner packages with inputs, secrets, outputs, assertions, and evidence.

## Problem

Even after real event capture exists, raw click/type/screenshot/terminal events are not enough. They must be normalized into durable semantic workflow steps that replay across app versions and machines where possible.

## Scope

1. Replace raw `recording.{action}` fallback steps with semantic actions.
2. Add backend-specific normalizers behind a shared interface:
   - web
   - native desktop
   - Java
   - terminal
   - remote/vision
3. Use `DesktopWorkflow` as the intermediate model.
4. Derive:
   - inputs from marked or repeated typed values
   - secrets from secret fields/prompts/markers
   - outputs from marked reads, copied values, terminal fields, OCR regions
   - assertions from stable visible text after submit/action
   - waits from observed transitions
5. Generate redaction rules for raw events and evidence metadata.
6. Score locator strength and emit open questions for weak/ambiguous locators.
7. Compile workflow to runner YAML using PR-54/55 schema.

## Normalization Examples

Web:

```text
click element role=button name=Calculate
=> web.click target role=button name=Calculate
```

Native:

```text
AX focused text field labelled "Amount", typed "100"
=> desktop.type_text target label=Amount value={{inputs.amount}}
```

Terminal:

```text
typed "123", pressed Enter, screen shows "Result: 2"
=> terminal.send_text "{{inputs.number_1}}"; terminal.extract_field result
```

Remote:

```text
clicked calibrated region and OCR result "2"
=> remote.click_region; remote.extract_text_region outputs.result
```

## Acceptance Criteria

- No normalized recording defaults to CRM-specific fields.
- Empty raw recordings cannot produce a "successful" runner without manual markers.
- Web, native, Java, terminal, and remote fixture recordings normalize into semantic runner YAML.
- Secret values never appear in runner YAML or unredacted evidence metadata.
- Weak locator cases produce open questions.
- Normalized runner can be tested through replay for each fixture.

## Test Plan

- Golden JSONL-to-runner tests per backend.
- Redaction tests.
- Open-question tests for ambiguous locators.
- Replay tests against fixture apps.
- Snapshot tests for generated YAML.

## Done Means

Recorded raw events from all backends become generic, replayable runners rather than low-level placeholders.

