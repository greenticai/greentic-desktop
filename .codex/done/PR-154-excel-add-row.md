## PR #154 - Add Excel header detection and search capabilities

### Goal

Build on PR-153 by adding practical read-only search operations for Excel workbooks. This PR is about finding data in existing workbooks; row writes and append operations belong in PR-155.

The filename mentions "add-row" from the original planning sequence, but this PR should remain read-only so the implementation can land in safe layers: read foundation, search, writes, then report generation.

### Capabilities

Add these capabilities to the `greentic.desktop.excel` adapter:

- `excel.detect_headers`
- `excel.search_cells`
- `excel.search_rows`
- `excel.validate_schema`

Expose the same capabilities through runner execution and MCP. Do not create a separate ad-hoc MCP server or tool registry.

### Tool 1: `excel.detect_headers`

Detect the likely header row and normalized column names.

Input:

```json
{
  "type": "object",
  "required": ["path", "sheet"],
  "properties": {
    "path": {"type": "string"},
    "sheet": {"type": "string"},
    "scan_rows": {"type": "integer", "default": 20, "minimum": 1, "maximum": 100}
  }
}
```

Output:

```json
{
  "path": "/Users/example/products.xlsx",
  "sheet": "Products",
  "detected": true,
  "header_row_number": 1,
  "confidence": 0.92,
  "columns": [
    {"name": "Product Name", "normalized": "product_name", "column_number": 1, "address": "A1"}
  ]
}
```

Use deterministic heuristics only:

- At least two non-empty cells.
- Mostly string cells.
- Followed by record-like rows.
- No duplicate normalized headers.
- Confidence based on simple observable signals.

No LLM should be used for header detection.

### Tool 2: `excel.search_cells`

Search all cells in a sheet for a text value.

Input:

```json
{
  "type": "object",
  "required": ["path", "sheet", "query"],
  "properties": {
    "path": {"type": "string"},
    "sheet": {"type": "string"},
    "query": {"type": "string"},
    "case_sensitive": {"type": "boolean", "default": false},
    "exact": {"type": "boolean", "default": false},
    "limit": {"type": "integer", "default": 50, "minimum": 1, "maximum": 500}
  }
}
```

Output:

```json
{
  "matches": [
    {
      "address": "A2",
      "row_number": 2,
      "column_number": 1,
      "value": {"type": "string", "value": "Wireless Mouse"}
    }
  ],
  "truncated": false
}
```

### Tool 3: `excel.search_rows`

Search rows using header-aware structured filters.

Input:

```json
{
  "type": "object",
  "required": ["path", "sheet", "filters"],
  "properties": {
    "path": {"type": "string"},
    "sheet": {"type": "string"},
    "header_row": {"type": "integer"},
    "filters": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["column", "op", "value"],
        "properties": {
          "column": {"type": "string"},
          "op": {
            "type": "string",
            "enum": [
              "equals",
              "not_equals",
              "contains",
              "starts_with",
              "ends_with",
              "greater_than",
              "greater_than_or_equal",
              "less_than",
              "less_than_or_equal",
              "is_empty",
              "is_not_empty"
            ]
          },
          "value": {}
        }
      }
    },
    "match": {"type": "string", "enum": ["all", "any"], "default": "all"},
    "limit": {"type": "integer", "default": 50, "minimum": 1, "maximum": 500}
  }
}
```

Output:

```json
{
  "header_row_number": 1,
  "matches": [
    {
      "row_number": 2,
      "cells": {
        "name": {"address": "A2", "type": "string", "value": "Wireless Mouse"},
        "price": {"address": "G2", "type": "number", "value": 24.99}
      }
    }
  ],
  "truncated": false
}
```

### Tool 4: `excel.validate_schema`

Validate that a sheet has expected columns before a workflow reads or writes it.

Input:

```json
{
  "type": "object",
  "required": ["path", "sheet", "required_columns"],
  "properties": {
    "path": {"type": "string"},
    "sheet": {"type": "string"},
    "header_row": {"type": "integer"},
    "required_columns": {"type": "array", "items": {"type": "string"}},
    "optional_columns": {"type": "array", "items": {"type": "string"}}
  }
}
```

Output:

```json
{
  "valid": true,
  "missing_columns": [],
  "matched_columns": [
    {"requested": "price", "actual": "Price", "column_number": 7}
  ]
}
```

### Planner And Prompting

Prompts that ask to "find", "search", "look up", "filter", or "return the row" from an Excel file should lower to:

1. `excel.detect_headers` when no header row is specified.
2. `excel.search_rows` for row-level result retrieval.
3. `excel.validate_schema` when the prompt names required fields.

Do not generate `activate_app`, `open_resource`, or `macos.copy_spreadsheet_row` for file-based spreadsheet search.

### Settings

Extend the Excel settings panel from PR-153 with:

- Default search result limit.
- Case-sensitive default.
- Whether fuzzy header matching is allowed.
- Maximum rows scanned for header detection.

### Tests

Add tests for:

- Header detection with clean headers.
- Header detection with title rows above headers.
- Duplicate header rejection.
- Cell search exact and contains modes.
- Row search with `all` and `any`.
- Numeric comparisons.
- Schema validation success and missing-column failure.
- Planner routes product lookup prompts to `excel.search_rows`.
- Existing macOS sample product lookup is migrated or duplicated to an Excel-adapter example.

### Acceptance Criteria

- A runner can look up a product in `examples/runners/sample.xlsx` without opening Microsoft Excel.
- The output includes nested structured fields such as `outputs.name`, `outputs.price`, or the repo's current nested output shape.
- Settings shows the Excel adapter as healthy when read/search configuration is valid.
- Desktop UI automation is not used for file-based Excel search.
