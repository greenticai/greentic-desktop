# PR-01 Core Runtime and CLI

## Goal

Create the Rust executable `greentic-desktop` and the CLI integration under `gtc desktop`.

The runtime is the stable host for all desktop automation, recording, replay, adapter loading, MCP serving, evidence collection and policy enforcement.

## Scope

### Crates

```text
crates/
  greentic-desktop-core/
  greentic-desktop-cli/
  greentic-desktop-config/
  greentic-desktop-session/
  greentic-desktop-runtime/
  greentic-desktop-telemetry/
```

### CLI Commands

```bash
gtc desktop info
gtc desktop init
gtc desktop config show
gtc desktop extension list
gtc desktop runner list
gtc desktop mcp serve --bind 127.0.0.1:8799
```

## Core Runtime Responsibilities

- Load runtime configuration
- Discover installed extensions
- Start or attach to desktop sessions
- Load runner packages
- Execute replay plans
- Capture evidence
- Enforce permissions
- Expose MCP tools
- Report telemetry
- Store run outcomes

## Runtime Config

```toml
[runner]
home = "~/.greentic/desktop"
registry_url = "https://runners.greentic.cloud"

[security]
require_signed_runners = true
allow_unsigned_drafts = true

[mcp]
bind = "127.0.0.1:8799"
transport = "streamable_http"

[evidence]
store = "~/.greentic/desktop/evidence"
```

## Acceptance Criteria

- `gtc desktop info` prints version, OS, installed adapters and registry path.
- `gtc desktop mcp serve` starts a valid MCP endpoint.
- Runtime can load local runner packages.
- Runtime refuses unsigned published runners if `require_signed_runners=true`.
- Runtime logs each tool call and replay execution.
