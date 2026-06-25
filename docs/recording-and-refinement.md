# Recording and Refinement

Greentic Desktop supports three ways to create a runner:

- **Assisted prompt**: describe the task and let the planner create draft steps.
- **Human demonstration**: capture what a person does.
- **Hybrid**: combine a prompt-generated outline with recorded desktop actions.

## Recording

A recording captures events such as clicks, fills, target metadata, values, timestamps, adapter names, and optional screenshot references.

Sensitive values are redacted before they become part of a runner package. For example, values that look like passwords, tokens, or secrets are replaced with a secret placeholder.

## Normalization

Recorded actions are normalized into stable runner steps. The goal is to capture the intent of an action, such as clicking a stable target, rather than relying only on raw coordinates.

## Prompt And Recording Together

Prompt steps and recorded steps can be merged. This lets a user describe the overall goal, demonstrate the fragile or application-specific parts, and end with a single runner package.

## Refinement

Refinement lets a user correct a runner without editing YAML manually. For example, a correction can say that the submit step should use the "Save" button at the bottom right. Greentic Desktop records a scoped diff for the changed step.

This is meant to keep runner maintenance approachable for operations teams while still producing reviewable changes.
