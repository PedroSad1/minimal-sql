use crate::error::Result;
use crate::types::QueryResult;

/// Build a portable SQL dump from query results (used for sqlite/pg/mysql/mssql backup).
pub fn sql_dump(table: &str, result: &QueryResult) -> Result<String> {
    crate::types::validate_ident(table)?;
    let mut out = String::new();
    out.push_str(&format!("-- Graphite dump for {table}\n"));
    for row in &result.rows {
        let values: Vec<String> = row.iter().map(sql_literal).collect();
        let cols = result
            .columns
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "INSERT INTO \"{table}\" ({cols}) VALUES ({});\n",
            values.join(", ")
        ));
    }
    Ok(out)
}

fn sql_literal(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "NULL".into(),
        serde_json::Value::Bool(v) => {
            if *v {
                "TRUE".into()
            } else {
                "FALSE".into()
            }
        }
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(text) => format!("'{}'", text.replace('\'', "''")),
        other => format!("'{}'", other.to_string().replace('\'', "''")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::QueryResult;
    use serde_json::json;

    #[test]
    fn dump_contains_insert() {
        let result = QueryResult {
            columns: vec!["id".into()],
            rows: vec![vec![json!(1)]],
            row_count: 1,
            truncated: false,
            enum_values: Vec::new(),
        };
        let dump = sql_dump("items", &result).unwrap();
        assert!(dump.contains("INSERT INTO"));
    }
}
