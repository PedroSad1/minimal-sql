use serde_json::Value;
use tokio_postgres::types::{to_sql_checked, IsNull, ToSql, Type};

#[derive(Debug, Clone)]
pub enum SqlVal {
    Null,
    Text(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl From<&Value> for SqlVal {
    fn from(value: &Value) -> Self {
        match value {
            Value::Null => SqlVal::Null,
            Value::Bool(v) => SqlVal::Bool(*v),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    SqlVal::Int(i)
                } else if let Some(f) = n.as_f64() {
                    SqlVal::Float(f)
                } else {
                    SqlVal::Text(n.to_string())
                }
            }
            Value::String(text) => SqlVal::Text(text.clone()),
            other => SqlVal::Text(other.to_string()),
        }
    }
}

impl ToSql for SqlVal {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut bytes::BytesMut,
    ) -> std::result::Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match self {
            SqlVal::Null => Ok(IsNull::Yes),
            SqlVal::Text(s) => s.to_sql(ty, out),
            SqlVal::Int(n) => n.to_sql(ty, out),
            SqlVal::Float(n) => n.to_sql(ty, out),
            SqlVal::Bool(v) => v.to_sql(ty, out),
        }
    }

    fn accepts(_ty: &Type) -> bool {
        true
    }

    to_sql_checked!();
}

pub fn json_to_mysql(value: &Value) -> mysql_async::Value {
    match value {
        Value::Null => mysql_async::Value::NULL,
        Value::Bool(v) => mysql_async::Value::Int(if *v { 1 } else { 0 }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                mysql_async::Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                mysql_async::Value::Double(f)
            } else {
                mysql_async::Value::Bytes(n.to_string().into_bytes())
            }
        }
        Value::String(text) => mysql_async::Value::Bytes(text.as_bytes().to_vec()),
        other => mysql_async::Value::Bytes(other.to_string().into_bytes()),
    }
}

pub fn mysql_to_json(value: mysql_async::Value) -> Value {
    match value {
        mysql_async::Value::NULL => Value::Null,
        mysql_async::Value::Int(n) => serde_json::json!(n),
        mysql_async::Value::UInt(n) => serde_json::json!(n),
        mysql_async::Value::Float(n) => serde_json::json!(n),
        mysql_async::Value::Double(n) => serde_json::json!(n),
        mysql_async::Value::Bytes(bytes) => {
            serde_json::json!(String::from_utf8_lossy(&bytes).to_string())
        }
        mysql_async::Value::Date(y, m, d, h, min, s, _) => {
            serde_json::json!(format!("{y:04}-{m:02}-{d:02} {h:02}:{min:02}:{s:02}"))
        }
        mysql_async::Value::Time(_, _, h, m, s, _) => {
            serde_json::json!(format!("{h:02}:{m:02}:{s:02}"))
        }
    }
}
