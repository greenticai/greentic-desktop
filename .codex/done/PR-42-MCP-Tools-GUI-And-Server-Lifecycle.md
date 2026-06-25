# PR-42 - MCP Tools GUI and Server Lifecycle

## Goal

Connect the `/mcp` page to the real MCP server state, published runner tools, tool testing, enable/disable controls, and copyable client configuration.

## User Outcome

A user can start MCP serving from the GUI, see which runner tools are available, test them, disable them, copy tool names, and configure AI clients or AWS WorkSpaces forwarding.

## Current State

- `/mcp` renders hard-coded MCP tools.
- Runtime has `mcp serve`, but it is a blocking CLI command.
- MCP crate models tool publishing, `tools/list`, `tools/call`, security policy, and evidence references.
- AWS WorkSpaces docs and models exist, but GUI does not expose configuration.

## Scope

1. Add managed MCP server lifecycle inside the GUI host.
2. Replace hard-coded tool cards with `/api/v1/mcp/tools`.
3. Add start/stop/restart.
4. Add test tool action.
5. Add enable/disable tool state.
6. Add client configuration helper for local MCP and AWS WorkSpaces.

## Backend Plan

### Managed MCP Process/Thread

`mcp serve` should remain available for CLI use, but GUI needs a non-blocking managed server:

```rust
McpServiceManager {
  start(bind) -> McpStatus
  stop() -> McpStatus
  restart(bind) -> McpStatus
  status() -> McpStatus
}
```

The GUI host and MCP server must not fight over the same port. Use separate binds:

- GUI: dynamic loopback port
- MCP: configured bind, default from runtime config

### Tool List

Return:

- tool name
- runner ID
- description
- status: `enabled | disabled | failing`
- version
- last call timestamp
- success rate if telemetry exists
- risk level
- input schema
- output schema

### Tool Test

Tool test should call through the same path as MCP `tools/call` where possible, with sample inputs supplied by UI.

### Enable/Disable

Persist disabled state in runtime config or a local MCP publication state file.

## Frontend Plan

- Server status card uses real `/api/v1/mcp/status`.
- Restart/stop buttons call backend and show result.
- Tool cards render real published tools.
- Copy button copies tool name and optionally JSON client config.
- Test action opens input modal and displays outputs/evidence.
- Disabled/failing states are visually distinct.

## AWS WorkSpaces Follow-Up

Expose a configuration panel that shows:

- local MCP bind URL
- AWS forwarded tool names
- link to `docs/aws-workspaces-mcp.md`
- whether forwarded MCP mode is configured

Do not claim AWS WorkSpaces API registration has happened unless the backend has actually performed it.

## Acceptance Criteria

- GUI can start, stop, and restart MCP service without blocking.
- `/mcp` lists real published runner tools.
- Testing an MCP tool uses backend call path and returns evidence.
- Disabled tools disappear from `tools/list` or are blocked at call time according to policy.
- Copy action copies the correct tool name/client config.

## Test Plan

- Backend tests for MCP manager lifecycle.
- Integration test: publish runner -> start MCP -> `tools/list` includes tool.
- Integration test: disabled tool is not callable.
- Frontend smoke test for MCP status and tool actions.

## Risks

- Running GUI HTTP and MCP HTTP servers in one process requires clear shutdown and port ownership.
- Long-lived MCP server threads must not keep stale runner state after publish/disable actions; add reload/invalidation.

