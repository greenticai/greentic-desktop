# PR-19 AWS WorkSpaces Integration

## Goal

Support AWS WorkSpaces as a runtime target without making the architecture AWS-specific.

## Patterns

### Pattern A: Runner installed inside Workspace

```text
AWS WorkSpace
  ├─ greentic-desktop-runner.exe
  ├─ adapters
  ├─ approved runner packages
  └─ MCP endpoint
```

### Pattern B: AWS managed MCP with forwarded Greentic tools

```text
MCP client
  ↓
AWS managed MCP endpoint
  ↓
WorkSpaces session
  ↓
forwarded Greentic runner tools
```

## Responsibilities

- Install runtime into golden image
- Pull approved runners from registry
- Install required adapters
- Start MCP endpoint
- Register available tools
- Run patch validation and business runners

## Commands

```bash
gtc desktop runner pull workspace.validate_after_patch --version stable
gtc desktop mcp serve --bind 127.0.0.1:8799
```

## WorkSpaces Use Cases

- Patch validation
- Golden image testing
- Application smoke testing
- Legacy app automation
- CRM data entry
- Contact centre desktop validation
- User onboarding

## Acceptance Criteria

- Runner can be installed in a WorkSpace.
- Approved runner can be pulled from registry.
- MCP server exposes tools.
- External MCP client can call a runner and receive evidence.
