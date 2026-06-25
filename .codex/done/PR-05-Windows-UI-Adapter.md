# PR-05 Windows UI Adapter

## Goal

Support Windows native applications including WinForms, WPF, VB6, .NET desktop applications and many legacy enterprise clients.

## Technology Options

- Microsoft UI Automation
- FlaUI
- WinAppDriver where available
- PowerShell fallback
- Screenshot/vision fallback for inaccessible controls

## Capabilities

```text
windows.open_app
windows.find_window
windows.find_element
windows.click_element
windows.type_text
windows.read_text
windows.read_window_tree
windows.assert_visible
windows.screenshot
windows.close_app
```

## Locator Strategies

```yaml
target:
  preferred:
    automation_id: CustomerSearchBox
  fallback:
    name: Customer Search
  class_name: TextBox
  visual_fallback:
    region: center
    nearby_text: Customer
```

## Recording

The adapter captures:

- Window title
- Process name
- Control tree
- Automation IDs
- Names
- Class names
- Clicked controls
- Text entry
- Screenshots
- Error dialogs

## VB/.NET Legacy Support

Many VB/.NET apps expose partial UI Automation metadata. The adapter should combine:

```text
automation_id + name + control type + relative position + visual fallback
```

## Acceptance Criteria

- Can open a native Windows app.
- Can find controls by Automation ID or Name.
- Can fill a form.
- Can detect error dialogs.
- Can replay recorded actions after a reboot.
