# Terminal TN3270 Adapter

Use `greentic.desktop.terminal-tn3270` for terminal and mainframe-style screen automation.

## Install

```bash
greentic-desktop extension install greentic.desktop.terminal-tn3270
greentic-desktop extension verify greentic.desktop.terminal-tn3270
greentic-desktop extension list
```

This is a sidecar extension. To inspect its launch metadata:

```bash
greentic-desktop extension sidecar greentic.desktop.terminal-tn3270
```

The current manifest launches:

```text
greentic-tn3270-adapter
```

## When To Use It

Use this adapter for:

- TN3270 or terminal-based workflows,
- mainframe menu navigation,
- fixed-screen data entry,
- reading values by row and column,
- extracting fields after text anchors,
- evidence snapshots of terminal screens.

## Capabilities

- `terminal.connect`
- `terminal.disconnect`
- `terminal.read_screen`
- `terminal.send_keys`
- `terminal.send_text`
- `terminal.type_text`
- `terminal.wait_for_screen`
- `terminal.assert_text`
- `terminal.extract_field`
- `terminal.capture_screen`

## Runner Planning

Plan a terminal runner from a prompt:

```bash
greentic-desktop runner plan \
  --prompt "Connect to the claims mainframe, look up a customer by customer id, and return the claim status" \
  --profile claims-mainframe \
  --out ./runners/mainframe.claim_status.draft.yaml
```

The prompt should name the host profile, the key input fields, the screen text that proves each step is complete, and the value to extract.

## Recording

Start a terminal recording session:

```bash
greentic-desktop record start \
  --name mainframe.claim_status \
  --profile claims-mainframe \
  --adapter greentic.desktop.terminal-tn3270 \
  --out ./recordings/mainframe.claim_status \
  --redact password,token \
  --secret-fields password
```

Useful recording annotations:

```bash
greentic-desktop record mark-input customer_id --session rec_123
greentic-desktop record mark-secret password --session rec_123
greentic-desktop record mark-output claim_status --session rec_123
greentic-desktop record add-assertion "CLAIM STATUS" --session rec_123
```

Normalise after stopping:

```bash
greentic-desktop record stop --session rec_123

greentic-desktop record normalise \
  --recording ./recordings/mainframe.claim_status/raw \
  --out ./runners/mainframe.claim_status.draft.yaml
```

## Locator Guidance

Terminal runners should rely on:

- screen text anchors,
- row and column field positions,
- stable menu names,
- expected screen headers,
- assertions such as `terminal.assert_text`,
- extraction steps such as `terminal.extract_field`.

Avoid timing-only waits. Prefer `terminal.wait_for_screen` with expected text.

## Use As An MCP Tool

After review and approval, expose the runner through MCP:

```bash
greentic-desktop mcp serve
```

An MCP client can call a terminal runner with business inputs:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "mainframe.claim_status",
    "arguments": {
      "customer_id": "C12345"
    }
  }
}
```

## Permissions And Notes

The built-in manifest requests `network.tenant`. Store terminal hosts and credentials as profile configuration or secrets; do not record passwords or tokens into runner steps.

Terminal automation must own the session. Local terminal fixtures use `portable-pty` and parse ANSI output through `vte`; SSH and TN3270 runners should use maintained transports behind the same owned-session boundary. Greentic should not claim support for recording arbitrary existing Terminal, iTerm, Windows Terminal, or emulator tabs.
