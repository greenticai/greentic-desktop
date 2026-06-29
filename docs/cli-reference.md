# CLI Reference

The repository currently provides two binaries:

- `greentic-desktop`
- `gtc`, with commands under `gtc desktop`

Published releases can be installed with:

```bash
cargo binstall greentic-desktop
```

Windows release zips also support direct click-to-run use: extract the archive
and double-click `greentic-desktop.exe` to open Automate Hub. The same binary
continues to support terminal commands. Public `cargo binstall` requires public
crates.io metadata and public release assets; private release assets require an
authenticated distribution path. See [Release And Installation](release.md).

## Automate Hub GUI

Starting `greentic-desktop` with no arguments initializes the runtime home, starts a
loopback GUI server on a free port, opens the default browser, prints the URL, and
keeps serving until interrupted.

```bash
greentic-desktop
```

The GUI can also be started explicitly:

```bash
greentic-desktop gui
greentic-desktop gui --bind 127.0.0.1:0
greentic-desktop gui --no-open
```

The GUI host also exposes a local JSON API under `/api/v1/*` for the embedded
frontend. Initial endpoints include `/api/v1/health`, `/api/v1/runtime/info`,
`/api/v1/setup/checklist`, `/api/v1/extensions/*`, `/api/v1/runners`,
`/api/v1/recordings`, and `/api/v1/mcp/status`.

`gtc desktop` remains an explicit CLI command surface and does not start the GUI
when no subcommand is supplied.

For browser-first setup, see [Automate Hub GUI](gui.md).

## Runtime Info

```bash
greentic-desktop info
```

Prints version, operating system, installed adapters, and local registry path.

## Initialize

```bash
greentic-desktop init
```

Creates the Greentic Desktop home directory, evidence directory, and extension directory.

## Show Configuration

```bash
greentic-desktop config show
```

Prints the default runtime configuration as TOML.

## Extensions

Install a built-in extension manifest:

```bash
greentic-desktop extension install greentic.desktop.playwright
greentic-desktop extension install playwright
```

List installed extensions:

```bash
greentic-desktop extension list
```

Search the extension store and list versions:

```bash
greentic-desktop extension search browser
greentic-desktop extension versions greentic.desktop.playwright
```

Show one installed extension:

```bash
greentic-desktop extension info greentic.desktop.playwright
```

Update, remove, enable, disable, or health-check an extension:

```bash
greentic-desktop extension update greentic.desktop.playwright
greentic-desktop extension remove greentic.desktop.playwright
greentic-desktop extension enable greentic.desktop.playwright
greentic-desktop extension disable greentic.desktop.playwright
greentic-desktop extension health greentic.desktop.playwright
```

Verify installed extensions:

```bash
greentic-desktop extension verify
```

Verify one built-in extension manifest:

```bash
greentic-desktop extension verify greentic.desktop.playwright
```

Show sidecar launch metadata for an installed sidecar extension:

```bash
greentic-desktop extension sidecar greentic.desktop.playwright
```

## Runners

List local runner packages:

```bash
greentic-desktop runner list
```

The runtime looks for `.gtpack` files under the local Greentic Desktop runner directory.

Package a reviewed runner through `greentic-pack`:

```bash
greentic-desktop runner pack crm.create_customer --out ./dist/crm.create_customer.gtpack
```

Greentic Desktop writes a temporary `answers.json` and invokes:

```bash
greentic-pack --answers /path/to/answers.json
```

Verify or install a package through `greentic-pack`:

```bash
greentic-desktop runner verify-pack ./dist/crm.create_customer.gtpack
greentic-desktop runner install-pack ./dist/crm.create_customer.gtpack
```

Create a draft runner from a prompt:

```bash
greentic-desktop runner plan \
  --prompt "Create CRM customer with company name and email and return customer id" \
  --profile local-crm \
  --out ./runners/crm.create_customer.draft.yaml
```

Preview the generated draft without writing a file:

```bash
greentic-desktop runner plan \
  --prompt-file ./prompt.md \
  --context ./desktop-context.json \
  --dry-run
```

The planner builds a structured LLM request, validates the returned runner draft, checks required capabilities against installed adapters, applies planner policy, and only then writes the draft.

## Recording

Start a recording session:

```bash
greentic-desktop record start \
  --name crm.create_customer \
  --profile local-crm \
  --adapter greentic.desktop.playwright \
  --out ./recordings/crm.create_customer
```

Manage the session:

```bash
greentic-desktop record pause --session rec_123
greentic-desktop record resume --session rec_123
greentic-desktop record status --session rec_123
greentic-desktop record stop --session rec_123
greentic-desktop record cancel --session rec_123
greentic-desktop record list
```

Normalise raw events into a draft runner and finalise it into the recording folder:

```bash
greentic-desktop record normalise \
  --recording ./recordings/crm.create_customer/raw \
  --out ./runners/crm.create_customer.draft.yaml

greentic-desktop record finalise \
  --recording ./recordings/crm.create_customer \
  --runner ./runners/crm.create_customer.draft.yaml
```

## MCP Server

Start the MCP endpoint on the default address:

```bash
greentic-desktop mcp serve
```

Start it on a specific address:

```bash
greentic-desktop mcp serve --bind 127.0.0.1:8799
```

## `gtc desktop` Form

Every command can also be called through the `gtc desktop` prefix:

```bash
gtc desktop config show
```
