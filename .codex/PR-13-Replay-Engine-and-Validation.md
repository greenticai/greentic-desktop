# PR-13 Replay Engine and Validation

## Goal

Execute approved or draft runner packages reliably and validate outcomes.

## Replay Flow

```text
load runner
  ↓
validate package
  ↓
resolve inputs and secrets
  ↓
prepare session
  ↓
execute steps
  ↓
capture evidence
  ↓
validate assertions
  ↓
extract outputs
  ↓
store outcome
```

## Step Execution

Each step has:

- ID
- Action
- Target
- Arguments
- Timeout
- Retry policy
- Evidence policy
- On-failure behaviour

## Validation Types

- Text visible
- Output exists
- Regex extraction succeeds
- DOM assertion
- UI Automation assertion
- Terminal screen contains text
- Screenshot matches baseline
- No error dialog
- App did not crash

## Safe Retry

Retries are allowed only for idempotent or explicitly marked safe steps.

```yaml
retry:
  max_attempts: 2
  safe: true
```

## Acceptance Criteria

- Runner can be replayed with provided JSON input.
- Validation pass/fail is deterministic.
- Outputs are returned as JSON.
- Failures include reason and evidence.
