## PR #153 - Add `greentic.desktop.excel` adapter with read-only workbook inspection

### Goal

Add a first-class Excel adapter/extension for local workbook inspection. Spreadsheet prompts such as "open this xlsx and read/search values" should use file-level Excel capabilities instead of driving Microsoft Excel through macOS/Windows/Linux desktop UI automation.

This PR is read-only. Write/update/report generation work belongs in PR-155 and PR-156.

### Architecture Decisions

- Add a workspace crate named `crates/greentic-desktop-excel`.
- Register the crate in root `Cargo.toml` workspace members.
- Add all new dependencies only in root `[workspace.dependencies]`; member crates must use `workspace = true`.
- Expose adapter id `greentic.desktop.excel`.
- Expose capabilities with dotted names:
  - `excel.list_sheets`
  - `excel.preview_sheet`
  - `excel.read_range`
- Add a built-in extension manifest for `greentic.desktop.excel` in `greentic-desktop-extension`.
- Register the adapter in GUI/runtime replay adapter registries so runner execution and MCP calls can resolve it.
- Add Settings > Adapter readiness support with accurate health/config status.
- Update planner and LLM capability context so `.xlsx`, `.xls`, workbook, sheet, spreadsheet, and Excel file-read prompts route to `greentic.desktop.excel`.
- Keep existing desktop Excel automation only as a fallback for visual/UI-only tasks. File-based workbook reads must not generate `macos.copy_spreadsheet_row` or app-level desktop steps once this adapter is installed.

### Dependencies

Add these to root `Cargo.toml` only:

```toml
calamine = "0.35"
thiserror = "2"
```

Use existing workspace `serde`, `serde_json`, `schemars`, and `jsonschema` entries.

### Crate Layout

Suggested layout:

```text
crates/greentic-desktop-excel/
  Cargo.toml
  src/lib.rs
  src/error.rs
  src/types.rs
  src/a1.rs
  src/workbook.rs
  src/adapter.rs
```

The crate should implement the existing `greentic_desktop_adapter::DesktopAdapter` contract and return normal `AdapterResult` / evidence-compatible output.

### Configuration

Add Excel adapter configuration surfaced in Settings:

- Allowed workbook roots, defaulting to the user home directory and the Greentic runtime examples directory.
- Maximum workbook size for local reads.
- Maximum preview rows/columns.
- Date/time formatting policy.
- Whether `.xls` is allowed. If `calamine` can read it but write support is unavailable, show this clearly.

Settings should show:

- Installed/available extension state.
- Adapter readiness and capabilities.
- Any missing configuration with actionable messages.

### Tool 1: `excel.list_sheets`

List available sheets in a workbook.

Input:

```json
{
  "type": "object",
  "required": ["path"],
  "properties": {
    "path": {"type": "string"},
    "include_hidden": {"type": "boolean", "default": false}
  }
}
```

Output:

```json
{
  "path": "/Users/example/customers.xlsx",
  "sheets": [
    {"name": "Customers", "index": 0, "visible": true}
  ]
}
```

If visibility cannot be reliably detected, return `visible: null`.

### Tool 2: `excel.preview_sheet`

Return the first N rows and columns of a sheet.

Input:

```json
{
  "type": "object",
  "required": ["path", "sheet"],
  "properties": {
    "path": {"type": "string"},
    "sheet": {"type": "string"},
    "max_rows": {"type": "integer", "default": 20, "minimum": 1, "maximum": 200},
    "max_columns": {"type": "integer", "default": 30, "minimum": 1, "maximum": 100}
  }
}
```

Output:

```json
{
  "path": "/Users/example/customers.xlsx",
  "sheet": "Customers",
  "row_count_returned": 20,
  "column_count_returned": 8,
  "rows": [
    {
      "row_number": 1,
      "cells": [
        {"address": "A1", "column_number": 1, "type": "string", "value": "Company"}
      ]
    }
  ]
}
```

### Tool 3: `excel.read_range`

Read an A1-style range from a sheet.

Input:

```json
{
  "type": "object",
  "required": ["path", "sheet", "range"],
  "properties": {
    "path": {"type": "string"},
    "sheet": {"type": "string"},
    "range": {"type": "string"},
    "header_row": {"type": "boolean", "default": false}
  }
}
```

Output:

```json
{
  "path": "/Users/example/customers.xlsx",
  "sheet": "Customers",
  "range": "A1:H20",
  "headers": ["Company", "Email"],
  "rows": [
    {
      "row_number": 2,
      "cells": [
        {"address": "A2", "column_number": 1, "type": "string", "value": "Acme Ltd"}
      ]
    }
  ]
}
```

### Cell Model

Normalize all read values into a stable JSON model:

```json
{
  "address": "B2",
  "row_number": 2,
  "column_number": 2,
  "type": "string|number|boolean|date|duration|formula|error|blank",
  "value": "display or JSON-native value",
  "raw": "optional raw debug value"
}
```

Formula cells must indicate whether the returned value is cached, formula text, or unavailable. Do not pretend formulas were recalculated.

### Security And File Access

- Expand `~` and relative paths through the existing safe path helpers.
- Reject paths outside configured allowed roots.
- Reject directories and unsupported file extensions.
- Return clear errors for missing files, password-protected workbooks, unreadable sheets, and malformed ranges.
- Do not open the workbook in Excel or any desktop app.

### Planner And Prompting

Update planner/router tests so prompts like these choose `greentic.desktop.excel`:

- "Ask for an xlsx file and return the row matching a search term."
- "Open the spreadsheet and read columns A through D."
- "List sheets in this workbook."

The generated runner should contain `required_adapters: ["greentic.desktop.excel"]` and `excel.*` steps, not `macos.*` desktop app steps.

### Tests

Add generated fixture workbooks and tests for:

- List sheets.
- Preview sheet limits.
- Read A1 ranges.
- Invalid range rejection.
- Missing sheet rejection.
- Date, number, boolean, blank, formula, and error cells.
- Path allowlist rejection.
- Adapter registry resolves `excel.read_range`.
- Settings readiness reports `healthy` when config is valid.
- Planner routes spreadsheet read prompts to `greentic.desktop.excel`.

### Acceptance Criteria

- A runner can read a local `.xlsx` file without opening Excel.
- Settings can install/configure/show readiness for `greentic.desktop.excel`.
- MCP runner execution can call Excel read capabilities and return structured nested outputs.
- Existing desktop app automation remains available, but spreadsheet file-read prompts prefer the Excel adapter.
