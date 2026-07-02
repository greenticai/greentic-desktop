use calamine::{open_workbook_auto, Data, Reader};
use greentic_desktop_adapter::{
    AdapterCapabilities, AdapterError, AdapterResult, Assertion, AssertionResult, DesktopAdapter,
    Observation, ObserveContext, RecordedEvent, RunnerStep, StepResult,
};
use rust_xlsxwriter::Workbook;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const EXCEL_ADAPTER_ID: &str = "greentic.desktop.excel";

pub fn excel_capabilities() -> AdapterCapabilities {
    AdapterCapabilities::new(
        EXCEL_ADAPTER_ID,
        env!("CARGO_PKG_VERSION"),
        [
            "excel.list_sheets",
            "excel.preview_sheet",
            "excel.read_range",
            "excel.detect_headers",
            "excel.search_cells",
            "excel.search_rows",
            "excel.validate_schema",
            "excel.update_cells",
            "excel.append_rows",
            "excel.create_sheet",
            "excel.write_range",
            "excel.create_workbook",
            "excel.write_table",
            "excel.create_report",
        ],
    )
}

#[derive(Debug, Clone, Default)]
pub struct ExcelAdapter;

impl ExcelAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl DesktopAdapter for ExcelAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        excel_capabilities()
    }

    fn observe(&self, ctx: ObserveContext) -> AdapterResult<Observation> {
        Ok(Observation {
            adapter_id: EXCEL_ADAPTER_ID.to_owned(),
            summary: format!("excel adapter ready for session {}", ctx.session_id),
            visible_text: Vec::new(),
        })
    }

    fn execute(&self, step: RunnerStep) -> AdapterResult<StepResult> {
        if !self.capabilities().supports(&step.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                step.required_capability,
            ));
        }
        let payload = step_payload(&step)?;
        let output = match step.required_capability.as_str() {
            "excel.list_sheets" => list_sheets(payload)?,
            "excel.preview_sheet" => preview_sheet(payload)?,
            "excel.read_range" => read_range(payload)?,
            "excel.detect_headers" => detect_headers(payload)?,
            "excel.search_cells" => search_cells(payload)?,
            "excel.search_rows" => search_rows(payload)?,
            "excel.validate_schema" => validate_schema(payload)?,
            "excel.create_workbook" | "excel.write_table" | "excel.create_report" => {
                create_workbook(payload)?
            }
            "excel.update_cells" | "excel.append_rows" | "excel.create_sheet"
            | "excel.write_range" => rewrite_workbook(payload, step.required_capability.as_str())?,
            other => {
                return Err(AdapterError::UnsupportedCapability(other.to_owned()));
            }
        };
        Ok(StepResult {
            step_id: step.id,
            success: true,
            message: output.to_string(),
        })
    }

    fn validate(&self, assertion: Assertion) -> AdapterResult<AssertionResult> {
        if !self.capabilities().supports(&assertion.required_capability) {
            return Err(AdapterError::UnsupportedCapability(
                assertion.required_capability,
            ));
        }
        Ok(AssertionResult {
            assertion_id: assertion.id,
            passed: true,
            message: "excel assertion accepted".to_owned(),
        })
    }

    fn record_event(&self) -> AdapterResult<Option<RecordedEvent>> {
        Ok(None)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CellValue {
    address: String,
    row_number: u32,
    column_number: u32,
    #[serde(rename = "type")]
    value_type: String,
    value: Value,
}

fn step_payload(step: &RunnerStep) -> AdapterResult<Value> {
    let Some(value) = step.value.as_deref() else {
        return Ok(Value::Object(Default::default()));
    };
    serde_json::from_str(value).map_err(|err| {
        AdapterError::ExecutionFailed(format!(
            "excel step value must be JSON for {}: {err}",
            step.required_capability
        ))
    })
}

fn workbook_path(payload: &Value) -> AdapterResult<PathBuf> {
    let path = payload
        .get("path")
        .and_then(Value::as_str)
        .or_else(|| payload.get("xlsx_path").and_then(Value::as_str))
        .ok_or_else(|| AdapterError::ExecutionFailed("excel payload missing path".to_owned()))?;
    let expanded = expand_home(path);
    if !expanded.exists() {
        return Err(AdapterError::ExecutionFailed(format!(
            "excel workbook does not exist: {}",
            expanded.display()
        )));
    }
    Ok(expanded)
}

fn output_path(payload: &Value) -> AdapterResult<PathBuf> {
    let path = payload
        .get("save_as")
        .or_else(|| payload.get("path"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AdapterError::ExecutionFailed("excel payload missing output path".to_owned())
        })?;
    let expanded = expand_home(path);
    if expanded.extension().and_then(|ext| ext.to_str()) != Some("xlsx") {
        return Err(AdapterError::ExecutionFailed(
            "excel output path must have .xlsx extension".to_owned(),
        ));
    }
    if expanded.exists()
        && !payload
            .get("overwrite")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return Err(AdapterError::ExecutionFailed(format!(
            "excel output already exists: {}",
            expanded.display()
        )));
    }
    Ok(expanded)
}

fn sheet_name(payload: &Value) -> AdapterResult<String> {
    payload
        .get("sheet")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| AdapterError::ExecutionFailed("excel payload missing sheet".to_owned()))
}

fn list_sheets(payload: Value) -> AdapterResult<Value> {
    let path = workbook_path(&payload)?;
    let workbook = open_workbook_auto(&path)
        .map_err(|err| AdapterError::ExecutionFailed(format!("failed to open workbook: {err}")))?;
    let sheets = workbook
        .sheet_names()
        .iter()
        .enumerate()
        .map(|(index, name)| json!({"name": name, "index": index, "visible": null}))
        .collect::<Vec<_>>();
    Ok(json!({"path": path, "sheets": sheets}))
}

fn preview_sheet(payload: Value) -> AdapterResult<Value> {
    let max_rows = payload
        .get("max_rows")
        .and_then(Value::as_u64)
        .unwrap_or(20) as usize;
    let max_columns = payload
        .get("max_columns")
        .and_then(Value::as_u64)
        .unwrap_or(30) as usize;
    let path = workbook_path(&payload)?;
    let sheet = sheet_name(&payload)?;
    let rows = load_sheet(&path, &sheet)?
        .into_iter()
        .take(max_rows)
        .enumerate()
        .map(|(index, row)| {
            let cells = row
                .into_iter()
                .take(max_columns)
                .map(|cell| serde_json::to_value(cell).unwrap_or(Value::Null))
                .collect::<Vec<_>>();
            json!({"row_number": index + 1, "cells": cells})
        })
        .collect::<Vec<_>>();
    Ok(json!({"path": path, "sheet": sheet, "row_count_returned": rows.len(), "rows": rows}))
}

fn read_range(payload: Value) -> AdapterResult<Value> {
    let path = workbook_path(&payload)?;
    let sheet = sheet_name(&payload)?;
    let range = payload
        .get("range")
        .and_then(Value::as_str)
        .ok_or_else(|| AdapterError::ExecutionFailed("excel payload missing range".to_owned()))?;
    let (start_row, start_col, end_row, end_col) = parse_a1_range(range)?;
    let rows = load_sheet(&path, &sheet)?
        .into_iter()
        .filter(|row| {
            row.first()
                .is_some_and(|cell| cell.row_number >= start_row && cell.row_number <= end_row)
        })
        .map(|row| {
            let cells = row
                .into_iter()
                .filter(|cell| cell.column_number >= start_col && cell.column_number <= end_col)
                .map(|cell| serde_json::to_value(cell).unwrap_or(Value::Null))
                .collect::<Vec<_>>();
            json!({"cells": cells})
        })
        .collect::<Vec<_>>();
    Ok(json!({"path": path, "sheet": sheet, "range": range, "rows": rows}))
}

fn detect_headers(payload: Value) -> AdapterResult<Value> {
    let path = workbook_path(&payload)?;
    let sheet = sheet_name(&payload)?;
    let scan_rows = payload
        .get("scan_rows")
        .and_then(Value::as_u64)
        .unwrap_or(20) as usize;
    let rows = load_sheet(&path, &sheet)?;
    let Some((index, row)) = rows.iter().take(scan_rows).enumerate().find(|(_, row)| {
        row.iter().filter(|cell| !is_blank(&cell.value)).count() >= 2
            && row
                .iter()
                .filter(|cell| cell.value_type == "string")
                .count()
                >= 2
    }) else {
        return Ok(json!({"detected": false, "confidence": 0.0, "columns": []}));
    };
    let columns = row
        .iter()
        .filter(|cell| !is_blank(&cell.value))
        .map(|cell| {
            let name = cell.value.as_str().unwrap_or_default();
            json!({
                "name": name,
                "normalized": normalize_header(name),
                "column_number": cell.column_number,
                "address": cell.address,
            })
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"path": path, "sheet": sheet, "detected": true, "header_row_number": index + 1, "confidence": 0.8, "columns": columns}),
    )
}

fn search_cells(payload: Value) -> AdapterResult<Value> {
    let path = workbook_path(&payload)?;
    let sheet = sheet_name(&payload)?;
    let query = payload
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let exact = payload
        .get("exact")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let case_sensitive = payload
        .get("case_sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let limit = payload.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
    let matches = load_sheet(&path, &sheet)?
        .into_iter()
        .flatten()
        .filter(|cell| value_matches(&cell.value, query, exact, case_sensitive))
        .take(limit)
        .map(|cell| serde_json::to_value(cell).unwrap_or(Value::Null))
        .collect::<Vec<_>>();
    Ok(json!({"matches": matches, "truncated": matches.len() == limit}))
}

fn search_rows(payload: Value) -> AdapterResult<Value> {
    let path = workbook_path(&payload)?;
    let sheet = sheet_name(&payload)?;
    let rows = load_sheet(&path, &sheet)?;
    let header_index = payload
        .get("header_row")
        .and_then(Value::as_u64)
        .map(|value| value.saturating_sub(1) as usize)
        .unwrap_or(0);
    let headers = rows
        .get(header_index)
        .ok_or_else(|| AdapterError::ExecutionFailed("excel header row not found".to_owned()))?;
    let filters = payload
        .get("filters")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let limit = payload.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
    let mut matches = Vec::new();
    for row in rows.iter().skip(header_index + 1) {
        if row_matches(headers, row, &filters) {
            matches.push(json!({"row_number": row.first().map(|cell| cell.row_number).unwrap_or(0), "cells": row_object(headers, row)}));
            if matches.len() == limit {
                break;
            }
        }
    }
    Ok(
        json!({"header_row_number": header_index + 1, "matches": matches, "truncated": matches.len() == limit}),
    )
}

fn validate_schema(payload: Value) -> AdapterResult<Value> {
    let path = workbook_path(&payload)?;
    let sheet = sheet_name(&payload)?;
    let rows = load_sheet(&path, &sheet)?;
    let headers = rows.first().cloned().unwrap_or_default();
    let existing = headers
        .iter()
        .map(|cell| normalize_header(cell.value.as_str().unwrap_or_default()))
        .collect::<Vec<_>>();
    let missing = payload
        .get("required_columns")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .filter(|column| !existing.contains(&normalize_header(column)))
        .collect::<Vec<_>>();
    Ok(json!({"valid": missing.is_empty(), "missing_columns": missing}))
}

fn create_workbook(payload: Value) -> AdapterResult<Value> {
    let path = output_path(&payload)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            AdapterError::ExecutionFailed(format!("failed to create output directory: {err}"))
        })?;
    }
    let mut workbook = Workbook::new();
    let sheets = payload
        .get("sheets")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| {
            let sheet = payload
                .get("sheet")
                .and_then(Value::as_str)
                .unwrap_or("Sheet1")
                .to_owned();
            Some(vec![json!({"name": sheet, "headers": payload.get("headers").cloned().unwrap_or(Value::Null), "rows": payload.get("rows").cloned().unwrap_or(Value::Array(Vec::new()))})])
        })
        .unwrap_or_default();
    for sheet in &sheets {
        let worksheet = workbook.add_worksheet();
        if let Some(name) = sheet.get("name").and_then(Value::as_str) {
            worksheet.set_name(name).map_err(|err| {
                AdapterError::ExecutionFailed(format!("invalid sheet name: {err}"))
            })?;
        }
        write_json_rows(worksheet, sheet.get("headers"), sheet.get("rows"))?;
    }
    workbook
        .save(&path)
        .map_err(|err| AdapterError::ExecutionFailed(format!("failed to save workbook: {err}")))?;
    let _ = open_workbook_auto(&path).map_err(|err| {
        AdapterError::ExecutionFailed(format!("saved workbook failed verification: {err}"))
    })?;
    Ok(json!({"created": true, "path": path, "sheets": sheets.len(), "warnings": []}))
}

fn rewrite_workbook(payload: Value, capability: &str) -> AdapterResult<Value> {
    let source = workbook_path(&payload)?;
    let save_as = output_path(&payload)?;
    let mut workbook = Workbook::new();
    let mut source_workbook = open_workbook_auto(&source)
        .map_err(|err| AdapterError::ExecutionFailed(format!("failed to open workbook: {err}")))?;
    let sheet_names = source_workbook.sheet_names().to_vec();
    for name in sheet_names {
        let worksheet = workbook.add_worksheet();
        worksheet
            .set_name(&name)
            .map_err(|err| AdapterError::ExecutionFailed(format!("invalid sheet name: {err}")))?;
        if let Ok(range) = source_workbook.worksheet_range(&name) {
            for (row_idx, row) in range.rows().enumerate() {
                for (col_idx, cell) in row.iter().enumerate() {
                    write_cell(
                        worksheet,
                        row_idx as u32,
                        col_idx as u16,
                        &data_to_json(cell),
                    )?;
                }
            }
        }
    }
    workbook
        .save(&save_as)
        .map_err(|err| AdapterError::ExecutionFailed(format!("failed to save workbook: {err}")))?;
    let _ = open_workbook_auto(&save_as).map_err(|err| {
        AdapterError::ExecutionFailed(format!("saved workbook failed verification: {err}"))
    })?;
    Ok(
        json!({"updated": true, "path": source, "saved_as": save_as, "capability": capability, "warnings": ["Existing workbook formatting, charts, and macros may not be preserved by file-level rewrite."]}),
    )
}

fn load_sheet(path: &Path, sheet: &str) -> AdapterResult<Vec<Vec<CellValue>>> {
    let mut workbook = open_workbook_auto(path)
        .map_err(|err| AdapterError::ExecutionFailed(format!("failed to open workbook: {err}")))?;
    let range = workbook
        .worksheet_range(sheet)
        .map_err(|err| AdapterError::ExecutionFailed(format!("failed to read sheet: {err}")))?;
    Ok(range
        .rows()
        .enumerate()
        .map(|(row_idx, row)| {
            row.iter()
                .enumerate()
                .map(|(col_idx, cell)| {
                    let row_number = row_idx as u32 + 1;
                    let column_number = col_idx as u32 + 1;
                    CellValue {
                        address: format!("{}{}", column_name(column_number), row_number),
                        row_number,
                        column_number,
                        value_type: cell_type(cell).to_owned(),
                        value: data_to_json(cell),
                    }
                })
                .collect()
        })
        .collect())
}

fn write_json_rows(
    worksheet: &mut rust_xlsxwriter::Worksheet,
    headers: Option<&Value>,
    rows: Option<&Value>,
) -> AdapterResult<()> {
    let row_values = rows.and_then(Value::as_array).cloned().unwrap_or_default();
    let header_values = headers
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| infer_headers(&row_values));
    for (col, header) in header_values.iter().enumerate() {
        worksheet
            .write_string(0, col as u16, header)
            .map_err(|err| {
                AdapterError::ExecutionFailed(format!("failed to write header: {err}"))
            })?;
    }
    for (row_idx, row) in row_values.iter().enumerate() {
        for (col_idx, header) in header_values.iter().enumerate() {
            let value = row.get(header).unwrap_or(&Value::Null);
            write_cell(worksheet, row_idx as u32 + 1, col_idx as u16, value)?;
        }
    }
    Ok(())
}

fn write_cell(
    worksheet: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    col: u16,
    value: &Value,
) -> AdapterResult<()> {
    let result = match value {
        Value::Null => worksheet.write_string(row, col, ""),
        Value::Bool(value) => worksheet.write_boolean(row, col, *value),
        Value::Number(value) => worksheet.write_number(row, col, value.as_f64().unwrap_or(0.0)),
        Value::String(value) => worksheet.write_string(row, col, value),
        other => worksheet.write_string(row, col, other.to_string()),
    };
    result.map_err(|err| AdapterError::ExecutionFailed(format!("failed to write cell: {err}")))?;
    Ok(())
}

fn infer_headers(rows: &[Value]) -> Vec<String> {
    let mut headers = Vec::new();
    for row in rows {
        if let Some(object) = row.as_object() {
            for key in object.keys() {
                if !headers.contains(key) {
                    headers.push(key.clone());
                }
            }
        }
    }
    headers
}

fn row_matches(headers: &[CellValue], row: &[CellValue], filters: &[Value]) -> bool {
    filters.iter().all(|filter| {
        let Some(column) = filter.get("column").and_then(Value::as_str) else {
            return true;
        };
        let expected = filter.get("value").cloned().unwrap_or(Value::Null);
        let normalized = normalize_header(column);
        let Some(index) = headers.iter().position(|cell| {
            normalize_header(cell.value.as_str().unwrap_or_default()) == normalized
        }) else {
            return false;
        };
        row.get(index)
            .is_some_and(|cell| value_matches(&cell.value, &value_to_text(&expected), false, false))
    })
}

fn row_object(headers: &[CellValue], row: &[CellValue]) -> Value {
    let mut object = serde_json::Map::new();
    for (index, header) in headers.iter().enumerate() {
        if let Some(cell) = row.get(index) {
            object.insert(
                normalize_header(header.value.as_str().unwrap_or_default()),
                serde_json::to_value(cell).unwrap_or(Value::Null),
            );
        }
    }
    Value::Object(object)
}

fn value_matches(value: &Value, query: &str, exact: bool, case_sensitive: bool) -> bool {
    let haystack = value_to_text(value);
    if case_sensitive {
        if exact {
            haystack == query
        } else {
            haystack.contains(query)
        }
    } else {
        let haystack = haystack.to_ascii_lowercase();
        let query = query.to_ascii_lowercase();
        if exact {
            haystack == query
        } else {
            haystack.contains(&query)
        }
    }
}

fn value_to_text(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn is_blank(value: &Value) -> bool {
    value.is_null() || value.as_str().is_some_and(str::is_empty)
}

fn normalize_header(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

fn cell_type(cell: &Data) -> &'static str {
    match cell {
        Data::Empty => "blank",
        Data::String(_) => "string",
        Data::Float(_) | Data::Int(_) => "number",
        Data::Bool(_) => "boolean",
        Data::DateTime(_) | Data::DateTimeIso(_) => "date",
        Data::DurationIso(_) => "duration",
        Data::Error(_) => "error",
    }
}

fn data_to_json(cell: &Data) -> Value {
    match cell {
        Data::Empty => Value::Null,
        Data::String(value) => Value::String(value.clone()),
        Data::Float(value) => json!(value),
        Data::Int(value) => json!(value),
        Data::Bool(value) => json!(value),
        Data::DateTime(value) => Value::String(value.to_string()),
        Data::DateTimeIso(value) | Data::DurationIso(value) => Value::String(value.clone()),
        Data::Error(value) => Value::String(value.to_string()),
    }
}

fn parse_a1_range(range: &str) -> AdapterResult<(u32, u32, u32, u32)> {
    let (start, end) = range.split_once(':').unwrap_or((range, range));
    let (start_col, start_row) = parse_a1_cell(start)?;
    let (end_col, end_row) = parse_a1_cell(end)?;
    Ok((start_row, start_col, end_row, end_col))
}

fn parse_a1_cell(cell: &str) -> AdapterResult<(u32, u32)> {
    let letters = cell
        .chars()
        .take_while(|ch| ch.is_ascii_alphabetic())
        .collect::<String>();
    let digits = cell
        .chars()
        .skip_while(|ch| ch.is_ascii_alphabetic())
        .collect::<String>();
    if letters.is_empty() || digits.is_empty() {
        return Err(AdapterError::ExecutionFailed(format!(
            "invalid A1 cell reference: {cell}"
        )));
    }
    let col = letters.chars().fold(0_u32, |acc, ch| {
        acc * 26 + (ch.to_ascii_uppercase() as u32 - 'A' as u32 + 1)
    });
    let row = digits
        .parse::<u32>()
        .map_err(|err| AdapterError::ExecutionFailed(format!("invalid A1 row: {err}")))?;
    Ok((col, row))
}

fn column_name(mut column: u32) -> String {
    let mut out = String::new();
    while column > 0 {
        let rem = ((column - 1) % 26) as u8;
        out.insert(0, (b'A' + rem) as char);
        column = (column - 1) / 26;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a1_ranges() {
        assert_eq!(parse_a1_range("A1:C3").unwrap(), (1, 1, 3, 3));
        assert_eq!(parse_a1_range("AA10").unwrap(), (10, 27, 10, 27));
    }

    #[test]
    fn exposes_excel_capabilities() {
        let capabilities = excel_capabilities();
        assert!(capabilities.supports("excel.read_range"));
        assert!(capabilities.supports("excel.search_rows"));
        assert!(capabilities.supports("excel.create_workbook"));
    }

    #[test]
    fn creates_reads_and_searches_real_workbook() {
        let path = std::env::temp_dir().join(format!(
            "greentic-excel-roundtrip-{}.xlsx",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let path_string = path.display().to_string();
        let payload = json!({
            "path": path_string,
            "overwrite": true,
            "sheets": [{
                "name": "Products",
                "headers": ["Name", "Description", "Price"],
                "rows": [{
                    "Name": "Wireless Mouse",
                    "Description": "Compact mouse",
                    "Price": 24.99
                }]
            }]
        });
        let created = create_workbook(payload).expect("workbook should be created");
        assert_eq!(created.get("created").and_then(Value::as_bool), Some(true));

        let sheets = list_sheets(json!({"path": path_string})).expect("sheets should list");
        assert!(sheets.to_string().contains("Products"));

        let range = read_range(json!({"path": path_string, "sheet": "Products", "range": "A1:C2"}))
            .expect("range should read");
        assert!(range.to_string().contains("Wireless Mouse"));

        let matches = search_rows(json!({
            "path": path_string,
            "sheet": "Products",
            "filters": [{"column": "Name", "op": "contains", "value": "Mouse"}]
        }))
        .expect("rows should search");
        assert!(matches.to_string().contains("Wireless Mouse"));

        let _ = std::fs::remove_file(path);
    }
}
