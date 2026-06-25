# PR-21 Business Process Automation with Desktop Runners

## Goal

Use recorded runners as production business tools, not only tests.

## Example: New Customer Onboarding

```text
AI web assistant collects customer data
  ↓
Greentic validates data
  ↓
MCP calls crm.create_customer
  ↓
MCP calls billing.create_account
  ↓
Greentic sends confirmation
  ↓
Evidence stored
```

## Flow Example

```yaml
flow: onboard_new_customer

steps:
  - collect_data:
      source: web_assistant

  - validate_inputs:
      required:
        - company_name
        - contact_email

  - call_runner:
      runner: crm.create_customer@1.2.0
      input:
        company_name: "{{request.company_name}}"
        contact_name: "{{request.contact_name}}"
        email: "{{request.email}}"
        phone: "{{request.phone}}"

  - call_runner:
      runner: billing.create_account@1.0.0
      input:
        customer_id: "{{steps.crm.outputs.customer_id}}"

  - send_confirmation_email
```

## Value

This lets Greentic automate apps that have no API.

## Acceptance Criteria

- Runner outputs can feed downstream flow steps.
- Human approval can be required before production submission.
- Evidence is stored for compliance.
