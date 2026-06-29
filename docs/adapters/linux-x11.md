# Linux X11 Adapter

Use `greentic.desktop.linux.x11` for Linux desktops running X11 with window management, screenshots, and input support.

## Install

```bash
greentic-desktop extension install greentic.desktop.linux.x11
greentic-desktop extension verify greentic.desktop.linux.x11
greentic-desktop extension list
```

This is a native extension. It does not expose sidecar launch metadata.

## When To Use It

Use this adapter for:

- Linux desktop apps under X11,
- GTK or Qt applications with accessibility metadata,
- window discovery and activation,
- form filling and button clicks,
- screenshots and screen assertions,
- workflows where XTest-style input is allowed.

## Capabilities

- `linux.find_window`
- `linux.read_window_tree`
- `linux.find_element`
- `linux.click_element`
- `linux.type_text`
- `linux.read_text`
- `linux.assert_visible`
- `linux.screenshot`
- `linux.activate_window`
- `linux.close_window`

## Runner Planning

Plan a Linux X11 runner:

```bash
greentic-desktop runner plan \
  --prompt "Open the Linux CRM desktop app, create a customer from company name and email, and return the customer id" \
  --profile linux-crm \
  --out ./runners/linux.crm_create_customer.draft.yaml
```

Include the app launcher, window title, field labels, and expected confirmation text.

## Recording

Start an X11 recording:

```bash
greentic-desktop record start \
  --name linux.crm_create_customer \
  --profile linux-crm \
  --adapter greentic.desktop.linux.x11 \
  --out ./recordings/linux.crm_create_customer \
  --redact text,password,email,token \
  --secret-fields password
```

Add semantic markers:

```bash
greentic-desktop record mark-input company_name --session rec_123
greentic-desktop record mark-input email --session rec_123
greentic-desktop record mark-output customer_id --session rec_123
greentic-desktop record add-assertion "Customer created" --session rec_123
```

Stop and normalise:

```bash
greentic-desktop record stop --session rec_123

greentic-desktop record normalise \
  --recording ./recordings/linux.crm_create_customer/raw \
  --out ./runners/linux.crm_create_customer.draft.yaml
```

## Locator Guidance

Prefer Linux accessibility and window metadata:

- accessible name,
- role,
- window title,
- app class,
- stable labels,
- screenshot fallback when metadata is incomplete.

Use `linux.read_window_tree` during review to inspect the available UI tree.

## Use As An MCP Tool

After approval, expose the runner and start MCP:

```bash
greentic-desktop mcp serve
```

Example MCP call:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "linux.crm_create_customer",
    "arguments": {
      "company_name": "Example Ltd",
      "email": "buyer@example.com"
    }
  }
}
```

## Permissions And Notes

The built-in manifest requests:

- `desktop.x11`
- `desktop.window_management`
- `desktop.screenshot`
- `desktop.input`

Use the Wayland adapter instead when the session is Wayland and global input/window control is restricted.

Screenshot evidence is captured through the shared `xcap` backend. Core X11 automation should move toward `x11rb` for window/input primitives and AT-SPI over `zbus` for element traversal instead of adding new shell command paths.
