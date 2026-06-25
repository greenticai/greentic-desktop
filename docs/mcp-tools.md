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

Automate Hub has an **MCP Tools** page for starting, stopping, restarting, enabling, disabling, testing, and copying published runner tools. It also shows local client configuration.

The CLI can also start the MCP endpoint:

```bash
greentic-desktop mcp serve
```

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
    "name": "crm.create_customer",
    "arguments": {
      "company_name": "Example Ltd",
      "email": "buyer@example.com"
    }
  }
}
```

The GUI-managed MCP service lists published and enabled runner tools. Disabled tools remain visible in the GUI but are not exposed through MCP `tools/list`.

## From Prompt Or Recording To Tool

Create a draft runner from a prompt:

```bash
greentic-desktop runner plan \
  --prompt "Create CRM customer with company name and email and return customer id" \
  --profile local-crm \
  --out ./runners/crm.create_customer.draft.yaml
```

Or create it from a recording:

```bash
greentic-desktop record start \
  --name crm.create_customer \
  --profile local-crm \
  --adapter greentic.desktop.playwright \
  --out ./recordings/crm.create_customer

greentic-desktop record normalise \
  --recording ./recordings/crm.create_customer/raw \
  --out ./runners/crm.create_customer.draft.yaml
```

After review, approval, packaging, and signing, the runner can be published as an MCP tool or converted into a forwarded tool for a managed desktop environment.

## Forwarded Tools

The repository also models forwarded MCP tools. These convert signed runner manifests and packages into local or AWS-forwarded tool descriptors so a runner can be called through the right desktop environment.
