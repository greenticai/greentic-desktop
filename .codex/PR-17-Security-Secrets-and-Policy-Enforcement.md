# PR-17 Security, Secrets and Policy Enforcement

## Goal

Ensure desktop runners are safe enough for production enterprise use.

## Security Principles

- Prompt creates drafts only.
- Published runners must be approved and signed.
- High-risk runners require approval.
- Secrets are never recorded into runner packages.
- Evidence redacts sensitive fields.
- Every run is auditable.

## Risk Levels

| Risk | Example | Default Policy |
|---|---|---|
| Low | Read-only validation | Auto |
| Medium | Create test/customer records | Approval for prod |
| High | Update production records | Human approval |
| Critical | Delete/payment/bulk update | Multi-approval |

## Policy Example

```yaml
risk_level: medium

permissions:
  read_screen: true
  type_text: true
  submit_forms: true
  delete_records: false
  payments: false

approval:
  production_required: true
  bulk_required: true

environments:
  allowed:
    - dev
    - staging
    - production
```

## Secrets

Use Greentic SecretsManager.

Runner references:

```yaml
username: "{{secrets.crm_user}}"
password: "{{secrets.crm_password}}"
```

Never store raw secrets in:

- runner YAML
- recordings
- screenshots
- logs
- LTM

## Acceptance Criteria

- Secrets are redacted from evidence.
- Published runners require signatures.
- Risk policies are enforced at MCP call time.
- Dangerous actions can be blocked.
