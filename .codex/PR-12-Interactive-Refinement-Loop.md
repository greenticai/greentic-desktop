# PR-12 Interactive Refinement Loop

## Goal

Allow users to improve a runner by prompting after each attempted execution.

## User Experience

```text
Greentic tried the step.
It clicked the wrong Save button.
User: "Use the Save button in the customer form, not the toolbar."
Greentic updates the runner and replays.
```

## Runtime Context Shown to User

- Current screenshot
- Step trace
- Last failure
- Observed screen text
- Available UI elements
- Suggested fix
- Diff of runner changes

## Correction Types

- Change selector
- Add wait
- Add assertion
- Change input mapping
- Add recovery step
- Mark step optional
- Add output extraction
- Restrict screen region
- Change adapter

## Runner Diff Example

```diff
- target:
-   text: Save
+ target:
+   text: Save
+   context: customer_form
+   region: bottom_right
```

## Acceptance Criteria

- User can correct failed steps using natural language.
- Runner is updated without rewriting unrelated steps.
- Diff is visible before applying.
- Replayed runner uses the correction.
