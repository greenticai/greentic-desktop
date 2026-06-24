# PR-10 Recording Engine and Portable Runner Package

## Goal

Capture a user performing a task and turn it into a portable, replayable runner package.

## Recording Modes

### Assisted Prompt Mode

User describes the task. Greentic attempts it. User corrects.

### Human Demonstration Mode

User performs the task once while Greentic records.

### Hybrid Mode

Greentic starts from a prompt, user takes over when needed, Greentic converts the interaction into a reusable runner.

## Recorded Data

- Input values
- Clicks
- Keystrokes
- Window/DOM/terminal state
- Screenshots
- Selectors
- Assertions
- Extracted outputs
- Timings
- Errors
- Human corrections

## Event Model

```json
{
  "event_type": "click",
  "timestamp": "...",
  "adapter": "greentic.desktop.playwright",
  "target": {
    "role": "button",
    "name": "Save",
    "css": "[data-testid='save']"
  },
  "screenshot_ref": "evidence://..."
}
```

## Normalisation

Raw events must be normalised.

```text
click x=520,y=430
  → click button labelled Save in customer form
```

## Acceptance Criteria

- Human demonstration can be captured.
- Prompt-generated steps can be merged with recorded steps.
- Recorded actions can be converted into runner YAML.
- Sensitive values can be redacted and replaced with input/secret placeholders.
