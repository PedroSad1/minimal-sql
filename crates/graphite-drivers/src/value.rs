use rusqlite::types::ValueRef;
use serde_json::{json, Value};

pub fn sqlite_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(n) => json!(n),
        ValueRef::Real(n) => json!(n),
        ValueRef::Text(bytes) => json!(String::from_utf8_lossy(bytes).to_string()),
        ValueRef::Blob(bytes) => json!(format!("\\x{}", hex(bytes))),
    }
}

pub fn json_to_sqlite(value: &Value) -> rusqlite::types::Value {
    match value {
        Value::Null => rusqlite::types::Value::Null,
        Value::Bool(v) => rusqlite::types::Value::Integer(if *v { 1 } else { 0 }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                rusqlite::types::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                rusqlite::types::Value::Real(f)
            } else {
                rusqlite::types::Value::Text(n.to_string())
            }
        }
        Value::String(text) => rusqlite::types::Value::Text(text.clone()),
        other => rusqlite::types::Value::Text(other.to_string()),
    }
}

pub fn json_display(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn pg_to_json(value: &tokio_postgres::types::Json<Value>) -> Value {
    value.0.clone()
}
