# PR-22 Forwarded Tool Builder

## Goal

Turn recorded desktop runners into high-level MCP tools and optionally forwarded tools for AWS WorkSpaces or other MCP environments.

## Input

A published runner:

```text
crm.create_customer@1.2.0
```

## Output

A callable MCP tool:

```text
crm.create_customer
```

or an AWS-forwarded style tool:

```text
forwarded___crm_create_customer
```

## Tool Metadata

- Name
- Description
- Input schema
- Output schema
- Risk level
- Evidence policy
- Required permissions
- Version

## Builder Flow

```text
published runner
  ↓
generate MCP tool descriptor
  ↓
register with local MCP server
  ↓
optionally forward to external MCP host
```

## Acceptance Criteria

- Published runner becomes a valid MCP tool.
- Tool schema matches runner input/output schema.
- Tool call runs the runner.
- Tool result includes structured output and evidence reference.
