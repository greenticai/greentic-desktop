# PR-03 Extension Manager and Sidecar Runtime

## Goal

Allow the Rust executable to install, manage and run adapters as extensions.

## Extension Types

### Native Extensions

Loaded directly into the Rust process using a stable ABI strategy.

Suitable for:

- Fast local integrations
- Simple Rust adapters
- Low overhead tools

### Sidecar Extensions

Run as child processes and communicate over JSON-RPC, MCP or local IPC.

Suitable for:

- Playwright Node adapter
- Java accessibility adapter
- Python OCR or vision tooling
- Vendor-specific desktop automation SDKs

## Extension Manifest

```toml
id = "greentic.desktop.playwright"
name = "Playwright Web Adapter"
version = "1.0.0"
runtime = "sidecar"
command = "node"
args = ["./index.js"]

[capabilities]
tools = [
  "web.goto",
  "web.click",
  "web.fill",
  "web.assert_visible",
  "web.extract_text"
]
```

## Commands

```bash
gtc desktop extension install greentic.desktop.playwright
gtc desktop extension install greentic.desktop.terminal-tn3270
gtc desktop extension list
gtc desktop extension update
gtc desktop extension verify
```

## Extension Sources

- Greentic public extension registry
- Tenant private extension registry
- Local airgapped extension bundle
- Git-backed customer repository

## Security

- Extensions must be signed.
- Extensions declare permissions.
- Sidecars run with least privilege where possible.
- Extension outputs are logged.
- Extension manifests are checked before installation.

## Acceptance Criteria

- Runtime can install a signed extension.
- Runtime can start a sidecar extension.
- Runtime can list extension capabilities.
- Runtime can refuse unsigned extensions in production mode.
