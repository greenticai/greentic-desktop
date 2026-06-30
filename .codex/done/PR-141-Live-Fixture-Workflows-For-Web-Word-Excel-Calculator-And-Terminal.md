# PR-141 - Live Fixture Workflows For Web Word Excel Calculator And Terminal

## Goal

Create a curated live workflow suite that exercises representative desktop automation paths end to end.

## User Outcome

Developers can validate Greentic against real apps and know which class of automation is currently working on their machine.

## Current Evidence

- Spreadsheet, Word, and calculator examples exposed real issues that normal tests missed.
- The examples are useful, but they need expected side effects and validation metadata.
- There is no one-command matrix that says which automation families are green.

## Scope

1. Add live fixture metadata for each workflow:
   - supported OS.
   - required app.
   - required permissions.
   - inputs.
   - expected outputs.
   - live assertions.
   - cleanup policy.
2. Add macOS fixtures:
   - Excel: create workbook, add one sheet, formula references another sheet, save/replace file.
   - Word: create document, enter text, select word, apply bold, save/replace file.
   - Calculator or browser calculator: calculate 10 * 10 and extract result.
3. Add web fixture:
   - local HTML form served by a test helper.
   - Playwright runner appends/submits data and extracts output.
4. Add terminal fixture:
   - portable PTY command that accepts inputs and prints structured output.
5. Add Linux and Windows fixture definitions with expected unsupported/setup states when the app is unavailable.
6. Add cleanup:
   - close or leave apps based on `--cleanup`.
   - remove temp files unless `--keep-artifacts`.

## File Targets

- `examples/runners/live/*.yaml`
- `examples/live-fixtures/*`
- `crates/greentic-desktop-test-harness/src/lib.rs`
- `docs/live-validation.md`

## Out of Scope

- Requiring Microsoft Office in CI.
- Hard-coding production runner planning to these apps.

## Acceptance Tests

1. `desktop validate-suite --suite examples/live-fixtures/macos.yaml` runs each available macOS fixture and reports pass/fail/skip.
2. Missing Word/Excel produces `missing_required_app`, not a generic step failure.
3. Excel fixture fails if a replace or format dialog remains open.
4. Word fixture fails if the saved document path is missing.
5. Web fixture runs without external network access.

## Done Means

There is a real live validation matrix for the app categories users keep testing manually.
