# MCP Tools

Greentic Desktop can expose approved desktop runners as MCP tools. This lets an assistant or MCP client ask for a business action, while Greentic Desktop handles the desktop replay behind the scenes.

## From Runner To Tool

A published runner becomes a tool with:

- a stable tool name,
- a human-readable description,
- an input schema reference,
- an output schema reference,
- a risk level,
- permission and approval settings,
- rate limits,
- evidence references.

Runner IDs are normalized into stable tool names so they can be safely listed and called by clients.

## Listing Tools

Automate Hub manages MCP from **My Runners**. Start, stop, or restart the managed MCP server there; each saved ready runner is the MCP tool contract. Run, edit, delete, and MCP calls use the same runner definition.

The standalone CLI `mcp serve` command is disabled until the runtime can load and execute installed `.gtpack` runner packages directly. Use the GUI-managed server for local assistant integrations.

The default address is:

```text
127.0.0.1:8799
```

From another terminal, an MCP-compatible client can request the tool list. A simple local HTTP check looks like this:

```bash
curl -fsS -X POST http://127.0.0.1:8799 \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

## Calling Tools

When a tool is called, Greentic Desktop checks:

- whether the tool is published and allowed,
- whether the caller has permission to use it,
- whether human approval is required,
- whether the environment is allowed,
- whether required inputs and secrets were supplied,
- whether the rate limit has been exceeded,
- whether the runner passes replay.

The result is JSON output plus an evidence URI. Failures are structured with a code, message, and optional evidence reference.

A runner tool call uses the standard MCP `tools/call` request shape:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "generic.resource_append",
    "arguments": {
      "resource_name": "contacts",
      "name": "Maarten",
      "email": "maarten@example.test"
    }
  }
}
```

The GUI-managed MCP service lists saved ready runners. Deleted or disabled runners are not exposed through MCP `tools/list`.

## From Prompt Or Recording To Tool

Create a draft runner from a prompt:

```bash
greentic-desktop runner plan \
  --prompt "Open a resource table, ask for resource_name, name, and email, append a row, save, and return saved_status" \
  --profile local-web \
  --out ./runners/generic.resource_append.draft.yaml
```

Or create it from a recording:

```bash
greentic-desktop record start \
  --name generic.resource_append \
  --profile local-web \
  --adapter greentic.desktop.playwright \
  --out ./recordings/generic.resource_append

greentic-desktop record normalise \
  --recording ./recordings/generic.resource_append/raw \
  --out ./runners/generic.resource_append.draft.yaml
```

After review, approval, packaging, and signing, the runner can be published as an MCP tool or converted into a forwarded tool for a managed desktop environment.

## Forwarded Tools

The repository also models forwarded MCP tools. These convert signed runner manifests and packages into local or AWS-forwarded tool descriptors so a runner can be called through the right desktop environment.
