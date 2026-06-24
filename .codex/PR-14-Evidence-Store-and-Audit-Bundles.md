# PR-14 Evidence Store and Audit Bundles

## Goal

Create audit-quality evidence for every runner build and replay.

## Evidence Bundle

```json
{
  "run_id": "run_123",
  "runner_id": "crm.create_customer",
  "runner_version": "1.2.0",
  "status": "success",
  "inputs_hash": "...",
  "outputs": {
    "customer_id": "CUST-49281"
  },
  "screenshots": [
    "before_submit.png",
    "after_success.png"
  ],
  "tool_trace": [],
  "started_at": "...",
  "completed_at": "..."
}
```

## Evidence Types

- Screenshots
- Annotated screenshots
- DOM snapshots
- Window tree snapshots
- Terminal screen buffers
- Tool traces
- Logs
- Error dialogs
- Output extraction proof

## Storage Model

```text
definition artifacts → Git/registry
evidence artifacts → object store
metadata → Greentic control plane/LTM
```

## Acceptance Criteria

- Every run produces an evidence bundle.
- Evidence bundle can be referenced from MCP result.
- Sensitive input values are redacted.
- Evidence is immutable once stored.
