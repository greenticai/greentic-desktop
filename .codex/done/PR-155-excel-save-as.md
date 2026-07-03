## PR #155 - Add safe Excel writes, append rows, and save-as semantics

### Goal

Add write support to the `greentic.desktop.excel` adapter so Greentic can update local `.xlsx` files without driving the Excel desktop app.

This PR covers controlled modification of existing workbooks:

- Update cells.
- Append rows using headers.
- Create sheets.
- Write rectangular ranges.
- Save modified workbooks safely.

Report/new-workbook generation belongs in PR-156.

### Dependencies

Evaluate a production-ready writer crate and add it only to root `[workspace.dependencies]`.

Preferred initial candidate:

```toml
umya-spreadsheet = "2.3"
```

The implementation must prove, with tests, what is preserved. If styles, formulas, charts, external links, or macros are not preserved reliably, document the limitation and return warnings. Initial production scope should be `.xlsx` only. Reject `.xlsm` writes unless macro preservation is explicitly proven.

### Capabilities

Add these capabilities to `greentic.desktop.excel`:

- `excel.update_cells`
- `excel.append_rows`
- `excel.create_sheet`
- `excel.write_range`

### Safety Rules

All write operations must be non-destructive by default.

- `save_as` is required unless `allow_in_place: true`.
- Existing output files are rejected unless `overwrite: true`.
- In-place writes require `allow_in_place: true`, `overwrite: true`, and a backup.
- Write via temporary file in the same directory, then atomic rename where possible.
- Use a lock file or advisory lock to avoid concurrent writes to the same workbook.
- Verify the saved workbook by reopening it with the read path from PR-153.
- Return `saved_as`, `backup_path`, `warnings`, and `verification` in output.
- Path checks must reuse the Excel adapter allowed-root settings.

### Tool 1: `excel.update_cells`

Update one or more cells and save the workbook.

Input:

```json
{
  "type": "object",
  "required": ["path", "sheet", "updates", "save_as"],
  "properties": {
    "path": {"type": "string"},
    "sheet": {"type": "string"},
    "updates": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["cell", "value"],
        "properties": {
          "cell": {"type": "string"},
          "value": {},
          "value_type": {"type": "string", "enum": ["auto", "string", "number", "boolean", "date", "formula"], "default": "auto"}
        }
      }
    },
    "save_as": {"type": "string"},
    "overwrite": {"type": "boolean", "default": false},
    "allow_in_place": {"type": "boolean", "default": false},
    "allow_formula_overwrite": {"type": "boolean", "default": false}
  }
}
```

If the target currently contains a formula, reject by default:

```json
{
  "error": {
    "code": "excel.formula_overwrite_not_allowed",
    "message": "Cell D17 contains a formula. Set allow_formula_overwrite=true to replace it."
  }
}
```

### Tool 2: `excel.append_rows`

Append rows by matching JSON fields to sheet headers.

Input:

```json
{
  "type": "object",
  "required": ["path", "sheet", "rows", "save_as"],
  "properties": {
    "path": {"type": "string"},
    "sheet": {"type": "string"},
    "header_row": {"type": "integer"},
    "rows": {
      "type": "array",
      "items": {"type": "object", "additionalProperties": true}
    },
    "save_as": {"type": "string"},
    "overwrite": {"type": "boolean", "default": false},
    "allow_in_place": {"type": "boolean", "default": false},
    "strict_columns": {"type": "boolean", "default": true}
  }
}
```

Behavior:

- Detect headers when `header_row` is omitted.
- Append after the last non-empty row in the table.
- Reject unknown fields when `strict_columns` is true.
- Preserve column order from the workbook.
- Return appended row numbers and cell addresses.

### Tool 3: `excel.create_sheet`

Create a new sheet in an existing workbook and save it.

Input:

```json
{
  "type": "object",
  "required": ["path", "sheet", "save_as"],
  "properties": {
    "path": {"type": "string"},
    "sheet": {"type": "string"},
    "headers": {"type": "array", "items": {"type": "string"}},
    "save_as": {"type": "string"},
    "overwrite": {"type": "boolean", "default": false},
    "if_exists": {"type": "string", "enum": ["error", "replace", "suffix"], "default": "error"}
  }
}
```

### Tool 4: `excel.write_range`

Write a rectangular range.

Input:

```json
{
  "type": "object",
  "required": ["path", "sheet", "start_cell", "rows", "save_as"],
  "properties": {
    "path": {"type": "string"},
    "sheet": {"type": "string"},
    "start_cell": {"type": "string"},
    "rows": {
      "type": "array",
      "items": {"type": "array", "items": {}}
    },
    "save_as": {"type": "string"},
    "overwrite": {"type": "boolean", "default": false}
  }
}
```

### Settings

Add write-specific Settings for the Excel adapter:

- Write operations enabled/disabled.
- Allowed write roots.
- Default backup directory.
- Overwrite policy.
- In-place write policy.
- Maximum rows/cells per write operation.
- Formula overwrite policy.

The Settings UI should make it clear when the adapter is installed but write operations are disabled by policy.

### Planner And Prompting

Prompts such as these should use `greentic.desktop.excel`:

- "Append a row to this spreadsheet with name and email."
- "Update the status column for matching invoice rows."
- "Create a new sheet with these results."

Generated runners should use `excel.append_rows`, `excel.update_cells`, or `excel.create_sheet`, not desktop UI steps.

When the prompt says "open Excel and edit what I see" without a file path, planner can ask a question for the workbook path or use desktop UI automation only after the user explicitly chooses a visual desktop workflow.

### Tests

Add tests for:

- Update cells with `save_as`.
- Reject missing `save_as`.
- Reject existing output without `overwrite`.
- Allow overwrite when configured.
- Reject in-place writes by default.
- Create backup for in-place writes.
- Append rows with detected headers.
- Reject unknown columns in strict mode.
- Create sheet and write headers.
- Write range.
- Reject `.xlsm` write unless preservation is proven.
- Reopen saved workbook and verify changed cells.
- Planner routes append/update spreadsheet prompts to `excel.*`.

### Acceptance Criteria

- Greentic can safely append/update rows in `.xlsx` files without opening Excel.
- The adapter never reports success unless the saved workbook exists and readback verification passes.
- Settings can configure and explain write policy.
- MCP runner outputs return nested structured data and evidence references.
