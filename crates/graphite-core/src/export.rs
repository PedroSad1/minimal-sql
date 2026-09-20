use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::error::Result;
use crate::types::QueryResult;

#[derive(Clone, Copy)]
pub enum QueryToFileFormat {
    Csv,
    Json,
}

pub fn export_csv(path: &Path, result: &QueryResult) -> Result<()> {
    let mut writer = csv::Writer::from_path(path).map_err(anyhow::Error::from)?;
    writer
        .write_record(&result.columns)
        .map_err(anyhow::Error::from)?;
    for row in &result.rows {
        let cells: Vec<String> = row.iter().map(value_to_string).collect();
        writer.write_record(&cells).map_err(anyhow::Error::from)?;
    }
    writer.flush().map_err(anyhow::Error::from)?;
    Ok(())
}

pub fn export_json(path: &Path, result: &QueryResult) -> Result<()> {
    let objects: Vec<serde_json::Value> = result
        .rows
        .iter()
        .map(|row| {
            let mut map = serde_json::Map::new();
            for (index, column) in result.columns.iter().enumerate() {
                map.insert(
                    column.clone(),
                    row.get(index).cloned().unwrap_or(serde_json::Value::Null),
                );
            }
            serde_json::Value::Object(map)
        })
        .collect();
    let json = serde_json::to_string_pretty(&objects).map_err(anyhow::Error::from)?;
    let mut file = File::create(path).map_err(anyhow::Error::from)?;
    file.write_all(json.as_bytes()).map_err(anyhow::Error::from)?;
    Ok(())
}

pub fn value_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::QueryResult;
    use serde_json::json;

    #[test]
    fn csv_and_json_roundtrip_files() {
        let dir = std::env::temp_dir();
        let csv_path = dir.join("graphite-export-test.csv");
        let json_path = dir.join("graphite-export-test.json");
        let result = QueryResult {
            columns: vec!["id".into(), "name".into()],
            rows: vec![vec![json!(1), json!("a")]],
            row_count: 1,
            truncated: false,
        };
        export_csv(&csv_path, &result).unwrap();
        export_json(&json_path, &result).unwrap();
        let csv = std::fs::read_to_string(&csv_path).unwrap();
        assert!(csv.contains("id,name"));
        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
        assert_eq!(parsed[0]["name"], "a");
    }
}
