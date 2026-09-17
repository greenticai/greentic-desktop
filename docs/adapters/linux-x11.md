# Linux X11 Adapter

Use `greentic.desktop.linux.x11` for Linux desktops running X11. Element automation goes through a native AT-SPI client (the `atspi` and `zbus` crates talking to the accessibility bus over D-Bus); window-manager control, focus typing and shortcuts use XTest tools.

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

- `linux.open_app`
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

## How Steps Execute

| Capability | Backend | Behaviour |
| --- | --- | --- |
| `linux.open_app` | process spawn + AT-SPI | `value` is an executable, an absolute path, or a `.desktop` entry (id or path, resolved through `XDG_DATA_HOME`/`XDG_DATA_DIRS`). When the target names a window that is already open it is reused; otherwise the program is spawned with `NO_AT_BRIDGE`/`GTK_A11Y` removed and the adapter waits for its accessible window (`GREENTIC_LINUX_ATSPI_LAUNCH_TIMEOUT_MS`, default 30 s). |
| `linux.find_window` | AT-SPI, `wmctrl` fallback | Waits for a top-level window whose name contains the title and scopes every later step to it. Falls back to `wmctrl` only when no accessibility bus exists. |
| `linux.find_element`, `linux.assert_visible` | AT-SPI | Polls until the locator resolves (`GREENTIC_LINUX_ATSPI_FIND_TIMEOUT_MS`, default 15 s). With no locator fields the step value is searched as a name. |
| `linux.click_element` | AT-SPI Action | Invokes `click`/`press`/`activate`. A target with only a `region` still uses `xdotool` coordinates. |
| `linux.type_text` | AT-SPI | EditableText when the control has it; otherwise (WebKitGTK entries) focus, select the existing text and synthesize the string through the AT-SPI device event controller. The value is read back and must match. Combo boxes are handled as selections, below. An untargeted step still types into the focused window through `xdotool`. |
| `linux.read_text` | AT-SPI | Reads Text contents or the name. A caption ending in `:` yields the next sibling's text. With `value: outputs.<name>` the result is emitted as `<name with spaces>: <text>`, which replay extracts into the output. |
| `linux.read_window_tree` | AT-SPI | Indented `role "name" #id` dump of the scoped window. |
| `linux.press_shortcut`, `linux.activate_window`, `linux.close_window`, `linux.screenshot` | `xdotool`, `wmctrl`, `xcap` | X11 only. |

Clicks and typing refuse to run until `linux.open_app` or `linux.find_window` has scoped the session to one window, so a locator such as `name: Close` can never act on another application.

### Locators

- `role` is a filter mapped onto AT-SPI roles: `button`, `textbox`, `combobox`, `spinbutton` (also matches `entry`), `heading`, `static text`, `checkbox`, `radio`, `option`, or any AT-SPI role name.
- `name`, `label` and `text` compare against the accessible name, Text contents and description, ignoring case, leading emoji/symbols and trailing `:`/`*`. `New Quote` matches `📄 New Quote`; `Company Name` matches `Company Name:*`. Exact matches beat substring matches.
- `automation_id` matches the object attribute `id`, which WebKitGTK sets from the DOM `id`. When a name/label is also given, the name decides and the id ranks candidates; if nothing matches the name, the id alone is tried (web toolkits often flatten a value's caption away while keeping the value's id).

### Choice controls

A value typed into a combo box selects the option whose visible text equals it, or — when the value is only digits — whose digits equal it (`2000000` selects `£2,000,000`). Options exposed inline are chosen through the Selection interface. WebKitGTK exposes a `<select>` with no options until it is opened: the adapter invokes its `select` action, activates the matching `table cell` in the GTK popup window of the same process, then reopens the popup to confirm that option is SELECTED before closing it.

### Diagnostics

Set `GREENTIC_LINUX_ATSPI_TRACE=1` to print each AT-SPI step, its duration and result to stderr. A `linux.read_window_tree` dump also lists why nodes were skipped.

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

After approval, expose the runner and start the managed MCP endpoint from Automate Hub **My Runners**.

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

Screenshot evidence is captured through the shared `xcap` backend. Element traversal and actions already use AT-SPI over `zbus`; the remaining `wmctrl`/`xdotool` paths (window activation/closing, shortcuts, untargeted typing, region clicks) are candidates for `x11rb` and stay confined to `crates/greentic-desktop-linux/src/lib.rs` by `ci/no_handrolled_scripting_check.sh`.

The target application must register with AT-SPI: `at-spi2-core` running in the session, and `NO_AT_BRIDGE`/`GTK_A11Y=none` unset for it. Keyboard synthesis goes through XTest, so typing into controls without EditableText needs the target window to accept X input focus.

## Live Verification

`examples/runners/aws-demo-linux-meridian-insurance.yaml` drives the Meridian Commercial Insurance Tauri app (WebKitGTK) end to end. Its live tests are `#[ignore]`d and run in a container, so they need Docker but no sudo and never touch the host desktop:

```bash
MERIDIAN_SRC=../aws-demo-meridian-insurance \
  ci/linux_atspi/run_live_check.sh --extraction
```

Add `DOCKER_DNS=1.1.1.1` where container DNS is broken. On a machine with an X11 session, AT-SPI and the app already built, the tests can run directly:

```bash
XDG_SESSION_TYPE=x11 GREENTIC_MERIDIAN_APP=/path/to/aws-demo-meridian-insurance \
  cargo test -p greentic-desktop-gui aws_demo_linux_live_replay -- --ignored --nocapture
```
