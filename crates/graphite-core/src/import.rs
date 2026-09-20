use std::path::Path;

use calamine::{open_workbook_auto, Data, Reader};
use serde_json::json;

use crate::error::{GraphiteError, Result};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedTable {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
}

pub fn import_csv_rows(path: &Path) -> Result<ImportedTable> {
    let mut reader = csv::Reader::from_path(path).map_err(anyhow::Error::from)?;
    let columns: Vec<String> = reader
        .headers()
        .map_err(anyhow::Error::from)?
        .iter()
        .map(|h| h.to_string())
        .collect();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(anyhow::Error::from)?;
        rows.push(record.iter().map(|cell| json!(cell)).collect());
    }
    Ok(ImportedTable { columns, rows })
}

pub fn import_json_rows(path: &Path) -> Result<ImportedTable> {
    let text = std::fs::read_to_string(path).map_err(anyhow::Error::from)?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(anyhow::Error::from)?;
    json_to_table(value)
}

pub fn import_xlsx_rows(path: &Path) -> Result<ImportedTable> {
    let mut workbook = open_workbook_auto(path).map_err(anyhow::Error::from)?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(|| GraphiteError::msg("xlsx has no sheets"))?
        .map_err(anyhow::Error::from)?;
    let mut rows_iter = range.rows();
    let header = rows_iter
        .next()
        .ok_or_else(|| GraphiteError::msg("xlsx is empty"))?;
    let columns: Vec<String> = header.iter().map(cell_to_string).collect();
    let mut rows = Vec::new();
    for row in rows_iter {
        rows.push(row.iter().map(cell_to_json).collect());
    }
    Ok(ImportedTable { columns, rows })
}

fn json_to_table(value: serde_json::Value) -> Result<ImportedTable> {
    let array = match value {
        serde_json::Value::Array(items) => items,
        other => {
            return Err(GraphiteError::msg(format!(
                "json import expects an array, got {other}"
            )))
        }
    };
    if array.is_empty() {
        return Ok(ImportedTable {
            columns: vec![],
            rows: vec![],
        });
    }
    let mut columns = Vec::new();
    if let serde_json::Value::Object(map) = &array[0] {
        columns = map.keys().cloned().collect();
    }
    let mut rows = Vec::new();
    for item in array {
        match item {
            serde_json::Value::Object(map) => {
                rows.push(
                    columns
                        .iter()
                        .map(|key| map.get(key).cloned().unwrap_or(serde_json::Value::Null))
                        .collect(),
                );
            }
            other => rows.push(vec![other]),
        }
    }
    Ok(ImportedTable { columns, rows })
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(text) => text.clone(),
        Data::Float(n) => n.to_string(),
        Data::Int(n) => n.to_string(),
        Data::Bool(v) => v.to_string(),
        Data::DateTime(dt) => format!("{dt:?}"),
        Data::DateTimeIso(text) | Data::DurationIso(text) => text.clone(),
        Data::Error(err) => format!("{err:?}"),
    }
}

fn cell_to_json(cell: &Data) -> serde_json::Value {
    match cell {
        Data::Empty => serde_json::Value::Null,
        Data::String(text) => json!(text),
        Data::Float(n) => json!(n),
        Data::Int(n) => json!(n),
        Data::Bool(v) => json!(v),
        other => json!(cell_to_string(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_import_reads_header_and_rows() {
        let path = std::env::temp_dir().join("graphite-import-test.csv");
        std::fs::write(&path, "id,name\n1,ada\n").unwrap();
        let table = import_csv_rows(&path).unwrap();
        assert_eq!(table.columns, vec!["id", "name"]);
        assert_eq!(table.rows.len(), 1);
    }

    #[test]
    fn json_import_reads_objects() {
        let path = std::env::temp_dir().join("graphite-import-test.json");
        std::fs::write(&path, r#"[{"id":1,"name":"ada"}]"#).unwrap();
        let table = import_json_rows(&path).unwrap();
        assert_eq!(table.columns.len(), 2);
        assert_eq!(table.rows.len(), 1);
    }
}
