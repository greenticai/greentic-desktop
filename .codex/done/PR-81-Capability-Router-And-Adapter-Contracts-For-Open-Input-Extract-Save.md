# PR-81 - Capability Router and Adapter Contracts for Open, Input, Extract, Save

## Goal

Define and implement generic capability contracts for desktop automation primitives: open target, attach window, create/open resource, locate field/cell/control, provide input, trigger command, save, observe, extract output, and assert state.

## User Outcome

Users do not need Excel-specific, calculator-specific, or CRM-specific runners. They describe a process and Greentic routes each semantic action to the best installed adapter for the target technology and OS.

## Current Evidence

- Native compile maps mostly to `macos.activate_app`, `windows.open_app`, `linux.find_window`, `type_text`, and `read_text`.
- There is no generic file/resource operation for “create if missing otherwise open”.
- There is no generic append-record/table-row action.
- There is no generic save command capability.
- Adapter IDs and capability names differ across GUI planner context and adapter crates.

## Problem

The product cannot satisfy generic desktop tasks without a stable semantic capability layer. “Spreadsheet” is just an example of a broader category: app-backed resource manipulation. The router needs to understand requested semantics and adapter availability before producing executable steps.

## Scope

1. Add semantic capability enum/types:
   - `open.target`
   - `open.resource`
   - `resource.create_if_missing`
   - `app.attach`
   - `ui.find`
   - `ui.input`
   - `ui.command`
   - `ui.save`
   - `ui.extract`
   - `resource.assert`
2. Map semantic capabilities to adapter-specific capabilities for:
   - web
   - macOS AX
   - Windows UIA
   - Linux X11/AT-SPI
   - Java Accessibility
   - terminal
   - vision fallback
3. Add router diagnostics:
   - unsupported target
   - missing permissions
   - missing adapter
   - ambiguous application
   - unsafe destructive action
4. Extend `DesktopWorkflow` compilation to compile semantic actions into adapter steps with fallback chains.
5. Update GUI setup to report missing capabilities required by the current runner.

## Acceptance Tests

1. Given installed macOS AX adapter and a native workflow, router maps open/input/save/extract to macOS capabilities.
2. Given no desktop adapter, the same workflow fails before run with actionable setup requirements.
3. Given a terminal workflow, router maps open/input/extract/save-like actions to terminal capabilities.
4. Vision fallback is only selected when semantic locators cannot be resolved and user/policy allows it.
5. No route may be selected solely because a keyword such as `calculator` appears.

