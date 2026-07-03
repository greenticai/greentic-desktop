# PR-151 - Java Access Bridge Backend Packaging and Setup

## Goal

Turn Java accessibility from an installed contract into a usable backend by packaging a real Java Access Bridge sidecar and wiring setup/health to it.

## User Outcome

For real Swing/AWT Java applications, users can install Java accessibility support, see a healthy Java adapter, run Java workflows, and record Java component interactions. Non-Java applications continue to route to native desktop adapters.

## Current Evidence

- `JavaDesktopAdapter` can call `GREENTIC_JAVA_ACCESS_BRIDGE_COMMAND`.
- The built-in extension manifest declares `greentic-java-accessibility-adapter`, but the app does not install/configure a working command.
- Health reports `not_implemented`, which hides the fact that what is missing is a concrete backend command.
- Prior PR-96 specified broad Java Access Bridge work, but there is still no packaged setup path.

## Scope

1. Implement or package a Java Access Bridge sidecar:
   - Windows: Java Access Bridge APIs.
   - macOS/Linux: supported Java accessibility bridge or explicit unsupported message if platform cannot provide it.
2. Define a stable JSON protocol for:
   - observe tree
   - find component
   - click/invoke
   - set value/type text
   - select item
   - read text
   - capture evidence
3. Install the sidecar into the Greentic runtime home when the Java extension is installed.
4. Persist the sidecar command in config/secrets-safe runtime settings instead of requiring a user shell env var.
5. Add setup diagnostics:
   - Java runtime present
   - Java Access Bridge enabled
   - sidecar executable present
   - fixture app reachable
6. Update planner routing so Java is selected only for explicit Java targets or detected Java process metadata.
7. Add a Java Swing fixture app and full replay/record tests.

## Acceptance Tests

1. Fresh install with Java extension but no backend reports `sidecar_missing` with setup steps, not `not_implemented`.
2. Setup installs/configures the sidecar command and health becomes `healthy`.
3. Java fixture runner enters text, clicks a button, and reads output through real accessibility APIs.
4. Java recording captures stable component locators from the fixture app.
5. Word/Excel/Office prompts never request Java capabilities unless the user explicitly selects Java.

