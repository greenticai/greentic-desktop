# Getting Started

Greentic Desktop runs desktop automation from a local runtime. The runtime keeps its own home folder with installed extensions, runner packages, configuration, and evidence.

## GUI-First Path

1. Install a release or build from this repository.
2. Start `greentic-desktop`, or double-click `greentic-desktop.exe` on Windows.
3. Open Automate Hub from the browser URL.
4. Complete the setup checklist.
5. Open **Settings** and install or verify the required extensions.
6. Open **Create** and generate a runner from a prompt, or record a task.
7. Test the runner and save it.
8. Open **My Runners** and publish the approved runner as an MCP tool.
9. Open **MCP Tools**, start the MCP service, and copy the client configuration.

The detailed browser workflow is in [Automate Hub GUI](gui.md). A release validation checklist is in [End-To-End Smoke Checklist](e2e-smoke.md).

## 1. Check The Runtime

Run:

```bash
greentic-desktop info
```

This prints the Greentic Desktop version, detected operating system, installed core adapter, and local registry path.

## 2. Initialize Local Storage

Run:

```bash
greentic-desktop init
```

By default, Greentic Desktop uses:

```text
~/.greentic/desktop
```

You can point it somewhere else by setting `GREENTIC_DESKTOP_HOME` before running the command.

## 3. Install An Adapter Extension

Adapters let Greentic Desktop interact with different kinds of applications. For example, to install the built-in Playwright web adapter manifest:

```bash
greentic-desktop extension install greentic.desktop.playwright
```

Then list installed extensions:

```bash
greentic-desktop extension list
```

## 4. Add Or Discover Runners

Runner packages use the `.gtpack` extension and are discovered from the local runner folder under the Greentic Desktop home directory.

```bash
greentic-desktop runner list
```

## 5. Record Your First Automation

Pick a small task that has a clear start and finish, such as creating a customer record, submitting a request, or collecting one report value.

Before recording, decide:

- what the runner should be called, such as `crm.create_customer`,
- which values should be inputs, such as `company_name` and `email`,
- which values are secrets, such as passwords or tokens,
- what proves the task worked, such as a confirmation message or created ID,
- what output should be returned to the caller.

Start a recording session:

```bash
greentic-desktop record start \
  --name crm.create_customer \
  --profile local-crm \
  --adapter greentic.desktop.playwright \
  --out ./recordings/crm.create_customer
```

During recording, Greentic Desktop captures the meaningful desktop actions: opening an app or page, clicking controls, filling fields, reading values, waiting for visible text, and taking screenshots when evidence is needed. Sensitive values are redacted before they become part of a runner.

Pause, resume, inspect, or stop the recording as needed:

```bash
greentic-desktop record pause --session rec_123
greentic-desktop record resume --session rec_123
greentic-desktop record status --session rec_123
greentic-desktop record stop --session rec_123
```

## 6. Convert The Recording Into A Runner

A recording becomes useful when it is normalized into a runner package. The runner package should contain:

- a stable runner ID, such as `crm.create_customer`,
- required inputs and secrets,
- the adapter capabilities it needs,
- replay steps with stable targets rather than raw screen coordinates,
- assertions that confirm the task reached the expected state,
- outputs returned to the caller,
- evidence rules for audit and troubleshooting.

When a `.gtpack` runner package is ready, place it in:

```text
~/.greentic/desktop/runners
```

If you set `GREENTIC_DESKTOP_HOME`, use that home folder instead. Confirm Greentic Desktop can see the runner:

```bash
greentic-desktop runner list
```

See [Runners](runners.md) for the runner package lifecycle and [Recording and Refinement](recording-and-refinement.md) for how recorded actions become stable replay steps.

You can also create a draft runner directly from a prompt:

```bash
greentic-desktop runner plan \
  --prompt "Create CRM customer with company name and email and return customer id" \
  --profile local-crm \
  --out ./runners/crm.create_customer.draft.yaml
```

## 7. Expose The Runner As An MCP Tool Forwarder

After a runner has been reviewed and approved, Greentic Desktop can expose it as an MCP tool. The forwarder is the tool-facing wrapper around the runner. It gives the runner:

- a stable tool name,
- a description an assistant can show to a user,
- an input schema,
- an output schema,
- permission and approval rules,
- rate limits,
- evidence settings.

The repository models both local and forwarded MCP tools, including AWS-forwarded names for runners that need to execute inside the right desktop environment. The current runtime MCP endpoint returns a generated tool list for the example published runner while the full runner registration commands are still being added.

## 8. Serve And Use MCP Tools

Greentic Desktop can serve approved runners as MCP tools:

```bash
greentic-desktop mcp serve
```

The default bind address is:

```text
127.0.0.1:8799
```

An MCP client can connect to that endpoint and list available tools. For a simple local check, start the server in one terminal and ask for the tool list from another:

```bash
curl -fsS -X POST http://127.0.0.1:8799 \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

When a client calls a runner tool, it sends the required inputs. Greentic Desktop checks permissions, approvals, secrets, environment policy, rate limits, and runner validity before replaying the desktop task. The result returns the runner outputs and an evidence reference.

In the full runner-tool flow, using the example `crm.create_customer` runner looks like this:

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

The MCP result contains either a structured failure or a successful response with the runner outputs and an evidence URI that can be attached to a ticket, audit record, or business workflow.

See [MCP Tools](mcp-tools.md) for how published runners become callable tools.

## Current Scope

The current repository exposes a practical CLI for initialization, extension manifests, runner discovery, and a minimal MCP endpoint. The broader runner creation, replay, approval, evidence, registry, deployment, and rollout flows are implemented as Rust models and tests that define the product behavior.
