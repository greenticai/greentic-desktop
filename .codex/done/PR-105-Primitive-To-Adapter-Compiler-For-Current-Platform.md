# PR-105 - Primitive to Adapter Compiler for Current Platform

## Goal

Compile typed primitives into concrete adapter steps for the current platform and target technology.

## User Outcome

The same workflow can run on macOS, Windows, Linux, Java, terminal, web, or remote desktop when the required adapter is installed, instead of emitting the wrong OS capabilities.

## Current Evidence

- A macOS user saw Windows capabilities in a Word runner.
- The compiler can lower some native workflows, but only from older fields and raw capabilities.
- The planner can generate raw capabilities that do not match the host platform.

## Scope

1. Add `WorkflowCompileTarget`:
   - `platform`
   - `display_server`
   - `installed_adapters`
   - `runtime_permissions`
   - `target_technology`
2. Add primitive compiler backends:
   - `compile_web_primitives`
   - `compile_macos_primitives`
   - `compile_windows_primitives`
   - `compile_linux_primitives`
   - `compile_java_primitives`
   - `compile_terminal_primitives`
   - `compile_vision_primitives`
3. Compile `OpenApp`:
   - macOS: `macos.activate_app`
   - Windows: `windows.open_app`
   - Linux X11: `linux.find_window` + `linux.activate_window`
4. Compile `SaveResourceAs` generically:
   - prefer OS-native Save dialog primitives.
   - fallback to menu/key sequence only when declared reliable for the adapter.
5. Compile `AssertResourceExists` to a replay output/assertion proof, not UI-only text extraction.
6. Make foreign native capabilities impossible for primitive-generated plans.
7. Return structured diagnostics:
   - missing adapter
   - missing permission
   - unsupported primitive
   - ambiguous target

## Out of Scope

- Full implementation of every adapter primitive.
- LLM prompt changes.

## Acceptance Tests

1. On macOS context, the document workflow compiles only to `macos.*` and file proof steps.
2. On Windows context, the same workflow compiles only to `windows.*` and file proof steps.
3. On Linux context, the same workflow compiles only to `linux.*` and file proof steps.
4. A primitive unsupported by an adapter fails before replay with `workflow.unsupported_primitive`.
5. No compiler path emits `windows.*` on non-Windows unless the target is explicitly remote Windows.

