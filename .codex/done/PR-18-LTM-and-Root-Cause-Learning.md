# PR-18 LTM and Root Cause Learning

## Goal

Store long-term operational memory for desktop runners.

## What LTM Stores

- Runner version
- App version
- Desktop image version
- Patch version
- Inputs hash
- Outputs
- Screenshots
- Failures
- Human corrections
- Root causes
- Fixes
- Approval decisions
- Final outcomes

## Use Cases

### Self-Healing

```text
Save button moved after CRM upgrade.
Previous fix: use automation_id SaveCustomerButtonV2.
```

### Patch RCA

```text
CRM validation failed after Windows KB update.
Similar historical failure was caused by missing WebView2 runtime.
```

### Runner Improvement

```text
This selector fails 30% of the time.
The fallback selector succeeds 98% of the time.
```

## Memory Model

```json
{
  "case_type": "runner_failure",
  "runner_id": "crm.create_customer",
  "app_version": "8.4",
  "failure": "Save button not found",
  "fix": "Use customer_form scoped Save button",
  "outcome": "resolved"
}
```

## Acceptance Criteria

- Every run outcome can be stored in LTM.
- Similar failures can be retrieved.
- Prompt planner can use LTM context.
- RCA summaries can be generated.
