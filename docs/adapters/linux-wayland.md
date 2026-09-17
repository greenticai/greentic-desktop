# Linux Wayland Compatibility Adapter

Use `greentic.desktop.linux.wayland` for constrained Linux Wayland sessions.

## Install

```bash
greentic-desktop extension install greentic.desktop.linux.wayland
greentic-desktop extension verify greentic.desktop.linux.wayland
greentic-desktop extension list
```

This is a native extension. It does not expose sidecar launch metadata.

## When To Use It

Use this adapter when:

- the desktop session is Wayland,
- global mouse and keyboard control is restricted,
- screenshots must go through desktop portals,
- accessibility trees are available through the desktop environment,
- the runner can use safe keyboard shortcuts instead of arbitrary pointer control.

Wayland intentionally limits desktop automation. This adapter is a compatibility path, not a drop-in replacement for X11 global control.

## Capabilities

- `linux.wayland.detect`
- `linux.wayland.portal_screenshot`
- `linux.wayland.accessibility_tree`
- `linux.wayland.assert_visible`
- `linux.wayland.safe_keyboard_shortcut`
- `linux.open_app`, `linux.find_window`, `linux.read_window_tree`, `linux.find_element`, `linux.click_element`, `linux.type_text`, `linux.read_text`, `linux.assert_visible`

The `linux.*` element capabilities use the same AT-SPI executor as the X11 adapter (see [Linux X11](linux-x11.md#how-steps-execute)). AT-SPI is D-Bus, not global input, so tree reads, element actions, EditableText writes and option selection work on Wayland. What does not:

- typing into a control without EditableText (WebKitGTK entries, for example) needs keyboard synthesis, which Wayland blocks. The step fails with that reason. For XWayland applications set `GREENTIC_LINUX_ATSPI_KEYBOARD=1` to allow it;
- `linux.press_shortcut`, `linux.activate_window`, `linux.close_window` and `linux.screenshot` are not offered.

The GUI reports `linux.wayland.*` accessibility as available only when an AT-SPI bus actually answers.

## Runner Planning

Plan a Wayland-safe runner:

```bash
greentic-desktop runner plan \
  --prompt "On Wayland, use portal screenshots and safe keyboard shortcuts to verify that the customer page is visible" \
  --profile linux-wayland-crm \
  --out ./runners/wayland.crm_check.draft.yaml
```

The prompt should explicitly avoid arbitrary clicks or typed text unless the target environment exposes a safe supported path.

## Recording

Start a Wayland recording:

```bash
greentic-desktop record start \
  --name wayland.crm_check \
  --profile linux-wayland-crm \
  --adapter greentic.desktop.linux.wayland \
  --out ./recordings/wayland.crm_check \
  --redact text,password,email,token \
  --secret-fields password
```

Use notes and assertions to capture what must be verified:

```bash
greentic-desktop record add-assertion "Customer page" --session rec_123
greentic-desktop record note "Wayland session only allows portal screenshot and safe shortcuts" --session rec_123
```

Stop and normalise:

```bash
greentic-desktop record stop --session rec_123

greentic-desktop record normalise \
  --recording ./recordings/wayland.crm_check/raw \
  --out ./runners/wayland.crm_check.draft.yaml
```

## Locator Guidance

Use Wayland-safe targets:

- portal screenshots,
- accessibility tree entries,
- visible text assertions,
- approved keyboard shortcuts,
- manual approval for restricted actions.

Do not design Wayland runners around global coordinates, arbitrary pointer clicks, or unrestricted keyboard injection.

## Use As An MCP Tool

After approval, expose the runner and start the managed MCP endpoint from Automate Hub **My Runners**.

Example MCP call:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "wayland.crm_check",
    "arguments": {}
  }
}
```

## Permissions And Notes

The built-in manifest requests:

- `desktop.wayland`
- `desktop.portal_screenshot`
- `desktop.accessibility`

If a workflow needs arbitrary click/type control, use an X11 session, a supported native app integration, or a managed environment that explicitly permits those operations.

Wayland screen and remote-control flows must go through desktop portals such as `ashpd`/XDG RemoteDesktop and ScreenCast. The adapter should keep returning explicit pending, denied, or unavailable states whenever the compositor has not granted those capabilities.
