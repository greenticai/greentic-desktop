# Recording and Refinement

Greentic Desktop supports three ways to create a runner:

- **Assisted prompt**: describe the task and let the planner create draft steps.
- **Human demonstration**: capture what a person does.
- **Hybrid**: combine a prompt-generated outline with recorded desktop actions.

The GUI path is usually easiest: open **Create**, choose recording, mark inputs and outputs during review, normalise the recording, test it, and finalise the runner. See [Automate Hub GUI](gui.md).

For target-specific recording ownership, permissions, blocked states, and local test commands, see [Recording Runbooks](recording-runbooks.md).

## Recording

A recording captures events such as clicks, fills, target metadata, values, timestamps, adapter names, and optional screenshot references.

Sensitive values are redacted before they become part of a runner package. For example, values that look like passwords, tokens, or secrets are replaced with a secret placeholder.

Start a recording session with a name, profile, adapter, and output folder:

```bash
greentic-desktop record start \
  --name generic.resource_append \
  --profile local-web \
  --adapter greentic.desktop.playwright \
  --out ./recordings/generic.resource_append \
  --redact text,password,email,token \
  --secret-fields password,api_key
```

The command writes a session manifest and an append-only raw event log under the recording folder. The session ID printed by `record start` is used for lifecycle commands:

```bash
greentic-desktop record pause --session rec_123
greentic-desktop record resume --session rec_123
greentic-desktop record status --session rec_123
greentic-desktop record stop --session rec_123
greentic-desktop record cancel --session rec_123
greentic-desktop record list
```

During a recording, you can add extra intent so the generated runner is easier to review:

```bash
greentic-desktop record mark-input resource_name --session rec_123
greentic-desktop record mark-input name --session rec_123
greentic-desktop record mark-secret password --session rec_123
greentic-desktop record mark-output saved_status --session rec_123
greentic-desktop record add-assertion "Saved row" --session rec_123
greentic-desktop record note "This dialog appears only when the resource does not exist yet" --session rec_123
```

## Normalization

Recorded actions are normalized into stable runner steps. The goal is to capture the intent of an action, such as clicking a stable target, rather than relying only on raw coordinates.

Convert raw events into a draft runner:

```bash
greentic-desktop record normalise \
  --recording ./recordings/generic.resource_append/raw \
  --out ./runners/generic.resource_append.draft.yaml
```

Finalise the recording by copying the reviewed draft runner back into the recording folder:

```bash
greentic-desktop record finalise \
  --recording ./recordings/generic.resource_append \
  --runner ./runners/generic.resource_append.draft.yaml
```

## Prompt And Recording Together

Prompt steps and recorded steps can be merged. This lets a user describe the overall goal, demonstrate the fragile or application-specific parts, and end with a single runner package.

Create a draft runner from a prompt:

```bash
greentic-desktop runner plan \
  --prompt "Open a resource table, ask for resource_name, name, and email, append a row, save, and return saved_status" \
  --profile local-web \
  --out ./runners/generic.resource_append.draft.yaml
```

Use `--dry-run` to inspect the generated draft without writing a file:

```bash
greentic-desktop runner plan \
  --prompt-file ./prompt.md \
  --context ./desktop-context.json \
  --dry-run
```

The planner builds a structured LLM request, validates the returned draft against the runner schema, checks required capabilities against installed adapters, applies planner policy, and only then writes the draft runner.

## Visible, Controlled, And Blocked Runs

Native desktop, Java, remote desktop, and visual fallback runs operate against the visible session or viewport the adapter can access. Web and terminal runs may use controlled contexts owned by Greentic Desktop, so they might not affect an unrelated browser tab or shell you already had open. Either way, a successful run must return declared outputs and an evidence reference.

If required permissions or event sources are missing, recording and replay should block with a concrete error such as `runner.capability_missing`, `runner.input_missing`, `runner.secret_missing`, or `runner.output_extraction_failed`. Do not treat a blocked or observe-only session as a successful recording.

## Refinement

Refinement lets a user correct a runner without editing YAML manually. For example, a correction can say that the submit step should use the "Save" button at the bottom right. Greentic Desktop records a scoped diff for the changed step.

This is meant to keep runner maintenance approachable for operations teams while still producing reviewable changes.

In Automate Hub, failed runner cards open a refinement panel. Enter the correction, preview the diff, apply it, then retest the runner and open the new evidence reference.
