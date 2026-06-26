# PR-54 - Generic DesktopWorkflow Model and Adapter Compilation

Goal: replace per-adapter workflow helper structs with one reusable workflow model that can describe desktop automations across web, native desktop, Java, terminal, vision, and remote workspace environments.

Problem
The repository now has separate generic-looking workflow types for macOS, Linux X11, Windows, Java, and Terminal. They all express the same high-level operation: open or attach to a target, locate inputs, provide values, submit actions, observe outputs, and validate. Keeping these models per adapter makes planner, recorder, replay, MCP, GUI, and future extensions duplicate logic.

Design
Add a shared workflow crate or module, preferably `greentic-desktop-workflow`, with:

- `DesktopWorkflow`
- `WorkflowTarget`
- `WorkflowInput`
- `WorkflowAction`
- `WorkflowOutput`
- `WorkflowAssertion`
- `WorkflowEvidencePolicy`
- `WorkflowCompileContext`
- `WorkflowCompileResult`

The generic model should be technology-neutral:

```text
workflow:
  id
  summary
  target:
    kind: web | native_app | java_app | terminal | vision | workspace
    open: optional launch/connect metadata
  inputs:
    name, type, required, secret, target, value_template
  actions:
    name, kind, target, value_template, risk
  outputs:
    name, type, extractor, required
  assertions:
    name, target, expected, capability_hint
```

Compilation
Add compiler functions that convert `DesktopWorkflow` into adapter-specific `RunnerStep` values:

- web: `web.goto`, `web.fill`, `web.click`, `web.extract_text`
- Windows: `windows.open_app`, `windows.find_window`, `windows.find_element`, `windows.type_text`, `windows.click_element`, `windows.read_text`
- macOS: `macos.activate_app`, `macos.find_window`, `macos.find_element`, `macos.type_text`, `macos.click_element`, `macos.read_text`
- Linux X11: `linux.find_window`, `linux.activate_window`, `linux.find_element`, `linux.type_text`, `linux.click_element`, `linux.read_text`
- Java: `java.find_window`, `java.find_component`, `java.type_text`, `java.click_component`, `java.read_text`
- Terminal: `terminal.connect`, `terminal.type_text`, `terminal.send_text`, `terminal.send_keys`, `terminal.wait_for_screen`, `terminal.extract_field`
- Vision: `vision.screenshot`, `vision.find_text`, `vision.click_region`, `vision.extract_text`

Migration
Keep current adapter workflow helpers temporarily as compatibility wrappers that internally build `DesktopWorkflow` and call the shared compiler. Remove the duplicate helpers after recorder, planner, replay, and tests use the shared model.

Acceptance criteria
One `DesktopWorkflow` can describe the current macOS Calculator fixture, Linux sample app fixture, Windows sample app fixture, Java sample app fixture, and terminal fixture.
Each fixture compiles to the expected adapter-specific `RunnerStep` sequence.
The per-adapter workflow runner tests are replaced or reduced to compiler tests.
No adapter contains app-specific workflow semantics.
Compilation errors identify missing launch target, locator, capability, input, output extractor, or unsupported workflow target kind.
