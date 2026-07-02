# PR-144 - Live Validation UX And Runbooks

## Goal

Expose live validation results in the GUI and document exactly how developers and users can diagnose automation failures.

## User Outcome

A failed run explains what happened and what to do next without requiring a developer to inspect logs or ask the user for screenshots.

## Current Evidence

- The user had to repeatedly provide screenshots showing dialogs still open.
- GUI errors often said only `step failed` or showed fallback internals.
- Setup and permission issues need to be separated from workflow bugs.

## Scope

1. Add GUI “Validate” action for runners:
   - prompts for inputs.
   - runs live validation.
   - shows pass/fail summary.
   - links to evidence.
2. Add validation details panel:
   - failed step.
   - expected assertion.
   - observed app/window/modal state.
   - screenshot.
   - suggested next action.
3. Add modal-specific guidance:
   - permission dialog.
   - replace dialog.
   - unsupported file format dialog.
   - unsaved changes dialog.
   - app not installed.
4. Add docs:
   - how to run live validation locally.
   - required permissions when launching from Terminal, VS Code, Cursor, or installed app.
   - how to interpret skip/fail/pass.
   - how to attach evidence to GitHub issues.
5. Add log/evidence retention settings:
   - default local path.
   - cleanup command.
   - redaction rules.

## File Targets

- `frontend/automate-hub/src/**`
- `crates/greentic-desktop-gui/src/lib.rs`
- `docs/live-validation.md`
- `docs/troubleshooting.md`

## Out of Scope

- Replacing the existing Run action.
- Sending evidence to a remote service.

## Acceptance Tests

1. GUI runner page has a Validate action separate from Run.
2. Validation failure displays modal text/buttons when a modal remains.
3. Validation success hides intermediate debug output and links to evidence.
4. Docs explain how to run macOS live validation from CLI-launched Greentic.
5. Evidence redaction avoids exposing secrets in GUI summaries.

## Done Means

The product gives users and developers a clear, reproducible way to validate and debug live desktop automation.
