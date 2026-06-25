# Windows UI Automation Adapter

Use `greentic.desktop.windows-ui` for native Windows desktop applications through UI Automation metadata.

## Install

```bash
greentic-desktop extension install greentic.desktop.windows-ui
greentic-desktop extension verify greentic.desktop.windows-ui
greentic-desktop extension list
```

This is a native extension. It does not expose sidecar launch metadata.

## When To Use It

Use this adapter for:

- Win32, WPF, WinForms, and other native Windows apps,
- workflows with stable Automation IDs,
- desktop apps with readable control names and control types,
- forms that need typed text and button clicks,
- reading text from windows and controls,
- checking for error dialogs after submit.

## Capabilities

- `windows.open_app`
- `windows.find_window`
- `windows.find_element`
- `windows.click_element`
- `windows.type_text`
- `windows.read_text`
- `windows.read_window_tree`
- `windows.assert_visible`
- `windows.screenshot`
- `windows.close_app`

## Runner Planning

Plan a Windows runner from a prompt:

```bash
greentic-desktop runner plan \
  --prompt "Open the desktop CRM app, create a customer from company name and email, and return the customer id" \
  --profile windows-crm \
  --out ./runners/windows.crm_create_customer.draft.yaml
```

Include the application name, expected window title, required inputs, and expected confirmation text in the prompt.

## Recording

Start a Windows recording session:

```bash
greentic-desktop record start \
  --name windows.crm_create_customer \
  --profile windows-crm \
  --adapter greentic.desktop.windows-ui \
  --out ./recordings/windows.crm_create_customer \
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
  --recording ./recordings/windows.crm_create_customer/raw \
  --out ./runners/windows.crm_create_customer.draft.yaml
```

## Locator Guidance

Prefer Windows UI Automation metadata:

- Automation ID,
- control name,
- control type,
- class name,
- window title,
- relative position only as fallback,
- visual fallback only when metadata is unavailable.

Use `windows.read_window_tree` during review to inspect available metadata and harden locators.

## Use As An MCP Tool

Expose the approved runner and start MCP:

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
    "name": "windows.crm_create_customer",
    "arguments": {
      "company_name": "Example Ltd",
      "email": "buyer@example.com"
    }
  }
}
```

## Permissions And Notes

The built-in manifest requests `desktop.ui_automation`. Run this adapter inside a desktop session where the target application is installed and visible to the user/session running Greentic Desktop.
