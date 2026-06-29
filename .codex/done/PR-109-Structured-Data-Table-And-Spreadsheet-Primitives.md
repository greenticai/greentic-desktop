# PR-109 - Structured Data, Table, and Spreadsheet Primitives

## Goal

Add generic primitives for spreadsheet/table workflows without hardcoding Excel, Numbers, LibreOffice, or Google Sheets.

## User Outcome

Users can prompt "open/create a spreadsheet, append a row with name and email, save it" and get a real, reusable workflow.

## Current Evidence

- Spreadsheet prompts are common and currently compile to generic text/click actions.
- The system has no first-class row/cell/table concepts.

## Scope

1. Add primitives:
   - `OpenTable { resource }`
   - `SelectCell { row, column }`
   - `SetCell { row, column, value }`
   - `AppendRow { values }`
   - `ReadCell { row, column }`
   - `ReadTable { range }`
   - `SaveTable`
2. Add structured target model:
   - row index
   - column index
   - column name
   - named range
3. Compiler lowering:
   - web tables via Playwright.
   - native desktop grids via accessibility roles.
   - fallback keyboard navigation only when table focus is proven.
4. Add CSV/TSV file fallback for workflows that do not require a GUI app.
5. Add file existence and file content proof for saved tables.

## Out of Scope

- Formula engines.
- App-specific spreadsheet APIs.

## Acceptance Tests

1. Prompt-generated spreadsheet workflow uses `AppendRow`, not arbitrary clicks.
2. Recording a row append normalizes into `AppendRow`.
3. CSV fallback can create `/tmp/example.csv` and verify contents.
4. Native table fixture app appends and reads a row through accessibility.
5. Output includes the saved file path and appended row proof.

