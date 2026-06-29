# Java Accessibility Adapter

Use `greentic.desktop.java-accessibility` for Java desktop applications exposed through accessibility metadata.

## Install

```bash
greentic-desktop extension install greentic.desktop.java-accessibility
greentic-desktop extension verify greentic.desktop.java-accessibility
greentic-desktop extension list
```

This is a sidecar extension. To inspect its launch metadata:

```bash
greentic-desktop extension sidecar greentic.desktop.java-accessibility
```

The current manifest launches:

```text
greentic-java-accessibility-adapter
```

## When To Use It

Use this adapter for:

- Swing, AWT, JavaFX, or other Java desktop apps,
- apps where component names, roles, or text are available,
- workflows that need component tree capture,
- forms with Java-specific accessibility metadata,
- cases where native platform adapters cannot see Java controls reliably.

## Capabilities

- `java.find_window`
- `java.find_component`
- `java.click_component`
- `java.type_text`
- `java.read_text`
- `java.assert_visible`
- `java.capture_tree`

## Runner Planning

Plan a Java runner:

```bash
greentic-desktop runner plan \
  --prompt "Open the Java billing app, search for an invoice by invoice id, and return the payment status" \
  --profile java-billing \
  --out ./runners/java.billing_status.draft.yaml
```

Mention the Java app name, window title, component labels, required inputs, and expected output.

## Recording

Start a Java accessibility recording:

```bash
greentic-desktop record start \
  --name java.billing_status \
  --profile java-billing \
  --adapter greentic.desktop.java-accessibility \
  --out ./recordings/java.billing_status \
  --redact password,token \
  --secret-fields password
```

Add useful markers:

```bash
greentic-desktop record mark-input invoice_id --session rec_123
greentic-desktop record mark-output payment_status --session rec_123
greentic-desktop record add-assertion "Payment Status" --session rec_123
```

Stop and normalise:

```bash
greentic-desktop record stop --session rec_123

greentic-desktop record normalise \
  --recording ./recordings/java.billing_status/raw \
  --out ./runners/java.billing_status.draft.yaml
```

## Locator Guidance

Prefer Java accessibility metadata:

- component name,
- role,
- visible text,
- keyboard shortcut,
- window title,
- visual fallback only if metadata is missing.

Use `java.capture_tree` during review to confirm the component tree exposes stable identifiers.

## Use As An MCP Tool

After approval, expose the runner and start the managed MCP endpoint from Automate Hub **My Runners**.

Example MCP call:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "java.billing_status",
    "arguments": {
      "invoice_id": "INV-1001"
    }
  }
}
```

## Permissions And Notes

The built-in manifest requests `desktop.java_accessibility`. Make sure the Java accessibility bridge or equivalent platform accessibility support is enabled for the target environment.

This adapter is for Java applications, not for generic desktop documents. Native apps such as Word, Excel, or platform file dialogs should route through the OS accessibility adapter unless an explicit app profile or process/accessibility metadata proves the target is Java. Direct JNI/JVM integration should only be added for a fixture that cannot be solved through OS accessibility metadata.
