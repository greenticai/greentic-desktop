# PR-24 End-to-End MVP Plan

## Goal

Define the smallest valuable version of Greentic Desktop.

## MVP Scope

### Runtime

- Rust executable
- Local config
- Local runner registry
- MCP server endpoint

### Adapters

- Playwright web adapter
- Windows UI adapter
- Terminal VT100/TN3270 adapter minimal
- Screenshot capture

### Builder

- Prompt-to-runner draft generation
- Interactive correction by prompting
- Replay until pass
- Publish local runner

### MCP

- Expose published runners as MCP tools
- Accept JSON input
- Return JSON output and evidence reference

### Evidence

- Screenshots
- Step trace
- Pass/fail report

### Workspace Worker

- Patch validation flow calling approved runners

## MVP Demo Scenario

1. User prompts: create CRM customer runner.
2. Greentic drafts runner.
3. Runner attempts web CRM.
4. User corrects Save button prompt.
5. Runner passes.
6. User publishes as `crm.create_customer`.
7. MCP client calls `crm.create_customer` with JSON data.
8. Runner creates customer and returns `customer_id`.
9. Patch validation flow calls `crm.validate_app` after Workspace patching.

## Success Criteria

- A non-developer can create a replayable runner by prompting.
- Runner can be refined without editing YAML manually.
- Runner can be published as MCP tool.
- Runner can be reused on another compatible Workspace.
- Evidence is captured for every run.
