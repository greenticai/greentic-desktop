# Evidence and Audit Trail

Evidence is the record of what happened during a runner execution. It supports troubleshooting, compliance review, and rollout decisions.

## What Evidence Represents

Evidence can include:

- screenshots or screen captures,
- step traces,
- redacted input hashes,
- output references,
- pass or fail reports,
- tool call traces,
- platform-specific replay paths,
- rollout validation reports.

Evidence is referenced by URI, for example:

```text
evidence://crm.create_customer/run-123
```

## Why Evidence Matters

Desktop automation often touches systems that were not designed for APIs. Evidence gives reviewers a way to understand:

- what runner was called,
- which inputs were used,
- which steps ran,
- which assertions passed or failed,
- what output was returned,
- why a run was blocked or failed.

## Redaction

Sensitive text is redacted before it is stored in evidence-oriented records. Secrets are handled separately from normal inputs, and secret-like text is replaced rather than written directly.

## Evidence In MCP Results

When a runner is called through MCP, successful and failed results include an evidence URI. A client can show that URI to a user or store it with the larger business process record.
