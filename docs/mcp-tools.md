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

The runtime has a minimal MCP endpoint that can answer tool listing requests. The CLI starts it with:

```bash
greentic-desktop mcp serve
```

The default address is:

```text
127.0.0.1:8799
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

## Forwarded Tools

The repository also models forwarded MCP tools. These convert signed runner manifests and packages into local or AWS-forwarded tool descriptors so a runner can be called through the right desktop environment.
