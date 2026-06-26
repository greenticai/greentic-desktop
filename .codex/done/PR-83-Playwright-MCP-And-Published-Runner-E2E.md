# PR-83 - Playwright MCP and Published Runner E2E

## Goal

Verify that every saved runner can be published as an MCP tool, the managed MCP server starts by default, tools are listed in the GUI, and MCP-backed run/test actions return typed outputs and evidence.

## Problem

Runners and MCP tools were merged conceptually. The application should behave as all-or-nothing: a saved runner has a corresponding MCP tool when published, delete removes both, and run/test uses the MCP-backed execution path by default.

## Scope

1. Add `e2e/mcp-published-runner.spec.ts`.
2. Start GUI and assert MCP service status:
   - default bind exists
   - service is started or can start from GUI
   - restart works
3. Publish seeded runners:
   - web calculator runner
   - native fake calculator runner
   - terminal fixture runner
4. Verify GUI:
   - Runners page shows publish status
   - MCP page lists the same tools
   - tool names are stable and human-readable
   - no separate orphan MCP-only tool row exists
5. Execute through GUI:
   - click Run on runner
   - input dialog appears
   - fill inputs
   - run returns output fields
   - evidence link opens
6. Execute through MCP protocol:
   - call `/mcp` or local MCP endpoint directly from test helper
   - list tools
   - call tool with same inputs
   - assert output matches GUI run
7. Delete:
   - delete runner with red button and confirmation
   - runner disappears
   - MCP tool disappears
   - underlying runner file is gone
8. Failure:
   - call with missing input
   - returns structured error
   - GUI displays error toast/banner
   - evidence/failure detail is available

## Acceptance Criteria

- MCP server starts by default in GUI mode or clearly starts automatically before tool execution.
- Published runner and MCP tool are one lifecycle.
- Run/test both use the same execution path.
- Delete removes runner and MCP tool together.
- MCP direct call and GUI run produce equivalent outputs.
- Errors are structured and visible.

## Test Plan

```bash
npm --prefix frontend/automate-hub run e2e -- --grep "@mcp"
cargo test -p greentic-desktop-mcp -p greentic-desktop-forwarded -p greentic-desktop-gui
```

## Risks

- MCP transport may evolve. Keep a protocol helper in one place and assert semantic tool behavior rather than raw HTTP formatting where possible.
