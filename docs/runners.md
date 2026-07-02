# Runners

A runner is a reusable description of a desktop task. It says what the task needs, what steps to perform, what inputs and secrets are required, what outputs are expected, and which desktop capabilities must be available.

## What A Runner Contains

A runner package includes:

- **ID and version**: the stable name of the task, such as `generic.resource_append`.
- **Mode**: whether it came from a prompt, a human demonstration, or both.
- **Inputs**: values supplied by the caller, such as `resource_name`, `name`, or `email`.
- **Secrets**: sensitive values resolved at run time, such as passwords or tokens.
- **Steps**: actions such as opening an app, clicking a button, typing text, reading a field, or asserting that something is visible.
- **Assertions**: checks that confirm the run reached the expected state.
- **Outputs**: values returned to the caller, such as `saved_status`.

## Portable Runners

Some desktop tasks need to work across more than one operating system. Portable runners can keep platform-specific launch details and locators for Windows, macOS, Linux X11, and Linux Wayland.

For example, the same logical step can open a generic desktop resource editor through:

- a Windows executable path,
- a macOS bundle ID,
- a Linux desktop file,
- and different element locators for each platform.

At replay time, Greentic Desktop selects the platform path that matches the current desktop.

## Runner Lifecycle

The intended lifecycle is:

1. Draft a runner from a prompt or demonstration.
2. Refine steps after user review.
3. Replay it against a compatible desktop.
4. Sign and publish it when approved.
5. Expose it as an MCP tool or use it inside a larger business workflow.
6. Keep evidence for each run.

Automate Hub exposes this lifecycle under **My Runners**: test or run a runner, inspect evidence, refine failed steps, approve high-risk actions, and publish the runner as an MCP tool.

## Generate A Draft Runner

Use prompt planning when you can describe the task before recording it:

```bash
greentic-desktop runner plan \
  --prompt "Open a resource table, ask for resource_name, name, and email, append a row, save, and return saved_status" \
  --profile local-web \
  --out ./runners/generic.resource_append.draft.yaml
```

Use recording when the task is easier to demonstrate:

```bash
greentic-desktop record start \
  --name generic.resource_append \
  --profile local-web \
  --adapter greentic.desktop.playwright \
  --out ./recordings/generic.resource_append

greentic-desktop record stop --session rec_123

greentic-desktop record normalise \
  --recording ./recordings/generic.resource_append/raw \
  --out ./runners/generic.resource_append.draft.yaml
```

Both paths produce a draft runner YAML file that should be reviewed before production use.

## Import And Export Runner YAML

Use Automate Hub **Create Runner > Provide a runner file** to import an existing `.yaml` or `.yml` runner from local disk, or import a runner source such as `oci://`, `store://`, or `repo://`.

Use **My Runners > Export YAML** to download the canonical YAML for a runner. Exports include runner definitions, inputs, outputs, secrets declarations, and steps, but never secret values or evidence bundles.

See [Runner Import And Export](import-export.md) for GUI and CLI examples.

## Make A Runner Discoverable

The current CLI discovers local `.gtpack` runner packages from the runtime runner folder:

```text
~/.greentic/desktop/runners
```

Place reviewed runner packages there, or use the folder selected by `GREENTIC_DESKTOP_HOME`. Then list them:

```bash
greentic-desktop runner list
```

Draft YAML and typed runner JSON files are useful for review and editing. Production `.gtpack` packages are built and verified by `greentic-pack`; Greentic Desktop does not implement a separate archive format.

```bash
greentic-desktop runner pack generic.resource_append --out ./dist/generic.resource_append.gtpack
greentic-desktop runner verify-pack ./dist/generic.resource_append.gtpack
greentic-desktop runner install-pack ./dist/generic.resource_append.gtpack
```

The `runner pack` command generates a temporary `answers.json` and delegates to `greentic-pack --answers answers.json`, so automated packaging uses the same package semantics as the Greentic pack tooling.

## Use A Runner

After a runner is approved and exposed as an MCP tool, start the managed MCP endpoint from Automate Hub **My Runners**.

For local desktop agents, stdio is the preferred MCP transport. When the GUI starts an HTTP MCP listener, requests must include the local session bearer token and must come from a localhost origin. Do not expose the HTTP MCP listener on an untrusted interface.

An MCP client can list tools and call the runner by name. A call for a generic resource append runner uses the normal MCP `tools/call` shape:

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

The response contains structured outputs, or a structured failure, plus an evidence reference when replay reaches the execution stage.

## Signed Published Runners

Published runners are expected to be signed. By default, the runtime refuses unsigned published runner packages while still allowing unsigned drafts during local authoring.

This gives teams a way to experiment locally while keeping production automation controlled.
