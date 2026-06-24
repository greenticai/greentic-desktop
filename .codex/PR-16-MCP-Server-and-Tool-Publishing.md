# PR-16 MCP Server and Tool Publishing

## Goal

Expose approved desktop runners as MCP tools.

## MCP Server

```bash
greentic-desktop-runner mcp serve --bind 127.0.0.1:8799
```

## Tool Generation

A runner becomes a tool:

```text
runner crm.create_customer
  → MCP tool crm.create_customer
```

Input schema comes from `inputs.schema.json`.

Output is returned as structured JSON.

## Runtime Flow

```text
MCP tools/call
  ↓
permission check
  ↓
load runner package
  ↓
execute replay
  ↓
capture evidence
  ↓
return outputs
```

## Security

- Tool-level permissions
- Runner risk-level enforcement
- Input validation
- Secret resolution policy
- Audit logging
- Rate limiting
- Optional human approval gates

## AWS Forwarded Tools

When used with AWS WorkSpaces MCP forwarding, Greentic can expose high-level tools such as:

```text
forwarded___crm_create_customer
forwarded___crm_validate_app
forwarded___workspace_validate_after_patch
```

## Acceptance Criteria

- `tools/list` returns all published allowed runners.
- `tools/call` executes the runner.
- Failed calls return structured failure and evidence reference.
- Tool names are stable.
