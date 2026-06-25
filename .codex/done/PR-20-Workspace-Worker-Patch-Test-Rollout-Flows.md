# PR-20 Workspace Worker Patch/Test/Rollout Flows

## Goal

Use desktop runners for patching, retesting, rollout and rollback.

## Flow

```text
select canary group
  ↓
snapshot baseline
  ↓
apply patches
  ↓
reboot/wait
  ↓
run validation runners
  ↓
compare evidence
  ↓
approve or rollback
```

## Example Flow

```yaml
flow: workspace_patch_validation

steps:
  - select_ring:
      ring: canary

  - patch:
      method: aws_ssm_patch_manager

  - wait_for_ready:
      timeout_minutes: 45

  - call_runner:
      runner: crm.validate_app@stable

  - call_runner:
      runner: finance.validate_invoice_app@stable

  - call_runner:
      runner: mainframe.lookup_customer@stable

  - evaluate_results:
      failure_threshold: 0

  - if_failed:
      - pause_rollout
      - create_ticket
      - notify_admin
      - rollback_canary

  - if_passed:
      - approve_next_ring
```

## Evidence Report

The flow should produce:

- Pass/fail by runner
- Screenshots
- Patch details
- Desktop versions
- Failed assertions
- Recommended action

## Acceptance Criteria

- Patch validation can call multiple runners.
- Failed runner blocks rollout.
- Evidence is attached to the rollout decision.
- Rollback can be triggered or recommended.
