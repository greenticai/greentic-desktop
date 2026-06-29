# macOS Accessibility Adapter

Use `greentic.desktop.macos.ax` for native macOS applications through Accessibility APIs.

## Install

```bash
greentic-desktop extension install greentic.desktop.macos.ax
greentic-desktop extension verify greentic.desktop.macos.ax
greentic-desktop extension list
```

This is a native extension. It does not expose sidecar launch metadata.

## When To Use It

Use this adapter for:

- AppKit or SwiftUI applications,
- native macOS desktop workflows,
- apps with Accessibility identifiers, titles, roles, or values,
- workflows that need window activation or close operations,
- screenshots as evidence.

## Capabilities

- `macos.find_app`
- `macos.find_window`
- `macos.read_window_tree`
- `macos.find_element`
- `macos.click_element`
- `macos.type_text`
- `macos.read_text`
- `macos.assert_visible`
- `macos.screenshot`
- `macos.activate_app`
- `macos.close_app`

## Runner Planning

Plan a macOS runner:

```bash
greentic-desktop runner plan \
  --prompt "Open the macOS CRM app, create a customer from company name and email, and return the customer id" \
  --profile macos-crm \
  --out ./runners/macos.crm_create_customer.draft.yaml
```

Include the app name or bundle ID, window title, field labels, and expected confirmation text.

## Recording

Start a macOS recording:

```bash
greentic-desktop record start \
  --name macos.crm_create_customer \
  --profile macos-crm \
  --adapter greentic.desktop.macos.ax \
  --out ./recordings/macos.crm_create_customer \
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
  --recording ./recordings/macos.crm_create_customer/raw \
  --out ./runners/macos.crm_create_customer.draft.yaml
```

## Locator Guidance

Prefer macOS Accessibility metadata:

- AX identifier,
- title,
- role,
- value,
- app name or bundle ID,
- window title,
- visual fallback only when accessibility metadata is unavailable.

Use `macos.read_window_tree` during review to inspect available element metadata.

## Use As An MCP Tool

After approval, expose the runner and start the managed MCP endpoint from Automate Hub **My Runners**.

Example MCP call:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "macos.crm_create_customer",
    "arguments": {
      "company_name": "Example Ltd",
      "email": "buyer@example.com"
    }
  }
}
```

## Permissions And Notes

The built-in manifest requests:

- `desktop.accessibility`
- `desktop.screen_recording`
- `desktop.input_monitoring`

Grant these permissions to the process that runs Greentic Desktop before recording or replaying native macOS workflows.

Screenshot evidence is captured through the shared `xcap` backend. Accessibility element traversal and element-targeted actions still depend on the macOS AX implementation; avoid app-specific scripts or AppleScript-only flows when designing new runner primitives.
