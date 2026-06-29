# PR-108 - Desktop Resource and Dialog Primitives for macOS, Windows, and Linux

## Goal

Implement real OS lowering for resource/document and dialog primitives across macOS, Windows, and Linux.

## User Outcome

Desktop automations can create, open, save, and verify files through real applications without hardcoded Word, Excel, or calculator logic.

## Current Evidence

- Saving a Word document fails because the workflow does not know how to drive a save dialog.
- File output proof is available, but the runner does not reliably create the file.

## Scope

1. macOS:
   - `ChooseMenu`
   - `PressKey`
   - `SaveResourceAs`
   - `SetDialogField`
   - `ChooseFile`
   - `ChooseFolder`
   - `AssertResourceExists`
2. Windows:
   - UIA menu/dialog lookup.
   - Save dialog filename/path entry.
   - `AssertResourceExists`.
3. Linux:
   - X11/AT-SPI menu/dialog lookup.
   - xdg/GTK/KDE save dialog handling where available.
   - Wayland limitation diagnostics when global automation is unavailable.
4. Add path handling:
   - expand `~`.
   - create parent directories when the primitive declares `ensure_parent`.
   - validate overwrite policy.
5. Do not hardcode Word/Excel:
   - target by generic document body/dialog/menu roles.
   - app-specific profiles may be data files, not Rust branches.

## Out of Scope

- Java and terminal primitive lowering.
- Remote desktop visual-only fallback.

## Acceptance Tests

1. macOS fixture app can create and save a document using primitives.
2. Windows fixture app can create and save a document using primitives.
3. Linux X11 fixture app can create and save a document using primitives.
4. `~/tests/test.docx` resolves to the user home path and verifies existence.
5. Missing parent directory either gets created or fails with a clear policy error.

