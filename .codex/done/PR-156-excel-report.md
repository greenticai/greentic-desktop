## PR #156 - Add Excel report generation with `rust_xlsxwriter`

### Goal

Add first-class generation of new `.xlsx` workbooks from structured data using the `greentic.desktop.excel` adapter.

This is separate from modifying existing workbooks in PR-155. This PR is for new reports, exports, result workbooks, and generated tables.

### Dependencies

Add this dependency only to root `[workspace.dependencies]`:

```toml
rust_xlsxwriter = "0.95"
```

The `greentic-desktop-excel` crate should consume it with `workspace = true`.

### Capabilities

Add these capabilities to `greentic.desktop.excel`:

- `excel.create_workbook`
- `excel.write_table`
- `excel.create_report`

Do not create a separate report adapter. Report generation belongs in the same Excel adapter so Settings, planner routing, MCP exposure, and security policy are consistent.

### Tool 1: `excel.create_workbook`

Create a new `.xlsx` workbook with one or more sheets.

Input:

```json
{
  "type": "object",
  "required": ["path", "sheets"],
  "properties": {
    "path": {"type": "string"},
    "overwrite": {"type": "boolean", "default": false},
    "sheets": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["name"],
        "properties": {
          "name": {"type": "string"},
          "headers": {"type": "array", "items": {"type": "string"}},
          "rows": {
            "type": "array",
            "items": {"type": "object", "additionalProperties": true}
          }
        }
      }
    }
  }
}
```

Output:

```json
{
  "created": true,
  "path": "/Users/example/report.xlsx",
  "sheets": [
    {"name": "Leads", "row_count": 10, "column_count": 4}
  ],
  "warnings": []
}
```

### Tool 2: `excel.write_table`

Create a workbook containing a single table.

Input:

```json
{
  "type": "object",
  "required": ["path", "sheet", "rows"],
  "properties": {
    "path": {"type": "string"},
    "sheet": {"type": "string", "default": "Sheet1"},
    "headers": {"type": "array", "items": {"type": "string"}},
    "rows": {
      "type": "array",
      "items": {"type": "object", "additionalProperties": true}
    },
    "overwrite": {"type": "boolean", "default": false},
    "autofilter": {"type": "boolean", "default": true},
    "freeze_header_row": {"type": "boolean", "default": true}
  }
}
```

Behavior:

- Use explicit headers when supplied.
- Infer headers deterministically when omitted: first row key order, then any new keys from later rows sorted by name.
- Write headers in row 1.
- Write data rows below.
- Apply autofilter/freeze options when requested.
- Use minimal formatting only: bold headers and reasonable column widths.

### Tool 3: `excel.create_report`

Create a formatted workbook from a report descriptor.

Input:

```json
{
  "type": "object",
  "required": ["path", "title", "sections"],
  "properties": {
    "path": {"type": "string"},
    "title": {"type": "string"},
    "overwrite": {"type": "boolean", "default": false},
    "sections": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["name", "rows"],
        "properties": {
          "name": {"type": "string"},
          "headers": {"type": "array", "items": {"type": "string"}},
          "rows": {
            "type": "array",
            "items": {"type": "object", "additionalProperties": true}
          }
        }
      }
    }
  }
}
```

Keep formatting deliberately simple in this PR. Do not add custom themes, images, charts, or branding unless a later PR defines templates.

### Value Writing Rules

Support:

| JSON value | Excel output |
| --- | --- |
| string | string |
| number | number |
| boolean | boolean |
| null | blank |
| array/object | compact JSON string |

### Safety Rules

- Refuse to overwrite an existing output unless `overwrite: true`.
- Ensure the parent directory exists and is inside configured output roots.
- Ensure output path has `.xlsx` extension.
- Write to a temp file, then move into place.
- Reopen generated files with `calamine` to verify readability and expected cell values.

### Settings

Extend Excel adapter Settings with:

- Allowed report output roots.
- Default report output directory.
- Maximum generated rows/cells.
- Whether overwrite is allowed.
- Default table options: autofilter, freeze header row.

### Planner And Prompting

Prompts like these should route to report generation:

- "Create an Excel report from these results."
- "Export the matching rows to an xlsx."
- "Generate a workbook with one tab per category."

Generated runners should use `excel.write_table`, `excel.create_workbook`, or `excel.create_report`.

### Tests

Add tests for:

- Create workbook with one sheet.
- Create workbook with multiple sheets.
- Write table with explicit headers.
- Write table with inferred headers.
- Reject overwrite when file exists and `overwrite` is false.
- Allow overwrite when `overwrite` is true and policy permits it.
- Reject non-`.xlsx` output path.
- Reject output outside configured roots.
- Read generated workbook back with `calamine`.
- Planner routes report/export prompts to `excel.*`.

### Acceptance Criteria

- Greentic can create new `.xlsx` files from structured runner data.
- Generated workbooks open in Excel-compatible tools.
- Settings can configure report generation policy.
- MCP calls can generate reports through the same runner execution path as other adapters.
