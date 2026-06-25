# PR-07 Terminal and Mainframe Adapters

## Goal

Support green-screen and terminal-based applications.

## Protocols

```text
VT100
VT220
TN3270
TN5250
SSH terminal
serial/console-like text UI
```

## Capabilities

```text
terminal.connect
terminal.disconnect
terminal.read_screen
terminal.send_keys
terminal.type_text
terminal.wait_for_screen
terminal.assert_text
terminal.extract_field
terminal.capture_screen
```

## Terminal Runner Example

```yaml
session_profile: mainframe_customer_system

steps:
  - action: terminal.connect
    args:
      protocol: tn3270
      host: "{{secrets.mainframe_host}}"

  - action: terminal.wait_for_screen
    args:
      contains: "LOGIN"

  - action: terminal.type_text
    value: "{{secrets.username}}"

  - action: terminal.send_keys
    keys: ENTER

  - action: terminal.wait_for_screen
    args:
      contains: "MAIN MENU"

  - action: terminal.type_text
    value: "CUST"

  - action: terminal.send_keys
    keys: ENTER

  - action: terminal.type_text
    value: "{{inputs.customer_id}}"

  - action: terminal.send_keys
    keys: ENTER

  - action: terminal.assert_text
    args:
      contains: "ACCOUNT STATUS"
```

## Screen Anchors

Terminal replay should rely on:

- Screen text anchors
- Row/column fields
- Function keys
- Stable menus
- Field labels

## Acceptance Criteria

- Can connect to a terminal host.
- Can record screen buffers.
- Can replay login and menu navigation.
- Can assert expected text.
- Can extract values by row/column or regex.
