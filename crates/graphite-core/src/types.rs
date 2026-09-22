use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    Postgres,
    Mysql,
    Sqlite,
    Sqlserver,
    Redis,
    Bigquery,
}

impl Dialect {
    pub fn quote_ident(&self, name: &str) -> crate::error::Result<String> {
        validate_ident(name)?;
        Ok(match self {
            Dialect::Postgres | Dialect::Sqlite | Dialect::Bigquery => format!("\"{name}\""),
            Dialect::Mysql => format!("`{name}`"),
            Dialect::Sqlserver => format!("[{name}]"),
            Dialect::Redis => name.to_string(),
        })
    }

    pub fn placeholder(&self, index: usize) -> String {
        match self {
            Dialect::Postgres | Dialect::Bigquery => format!("${}", index + 1),
            Dialect::Mysql | Dialect::Sqlite => "?".to_string(),
            Dialect::Sqlserver => format!("@P{}", index + 1),
            Dialect::Redis => String::new(),
        }
    }
}

pub fn validate_ident(name: &str) -> crate::error::Result<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '.')
    {
        return Err(crate::error::GraphiteError::InvalidIdentifier(
            name.to_string(),
        ));
    }
    Ok(())
}

pub fn dialect_for(connection_type: &str) -> crate::error::Result<Dialect> {
    Ok(match connection_type {
        "postgresql" | "cockroachdb" | "redshift" | "greengage" => Dialect::Postgres,
        "mysql" | "mariadb" | "tidb" | "starrocks" | "bedrock" => Dialect::Mysql,
        "sqlite" => Dialect::Sqlite,
        "sqlserver" => Dialect::Sqlserver,
        "redis" => Dialect::Redis,
        "bigquery" => Dialect::Bigquery,
        other => return Err(crate::error::GraphiteError::UnsupportedType(other.into())),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub private_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ConnectionConfig {
    pub connection_type: String,
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub default_database: Option<String>,
    pub filename: Option<String>,
    pub ssl: bool,
    pub ssh: Option<SshConfig>,
    pub project_id: Option<String>,
    pub dataset: Option<String>,
    pub service_account_json: Option<String>,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableOrView {
    pub name: String,
    pub schema: Option<String>,
    pub entity_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableColumn {
    pub column_name: String,
    pub data_type: String,
    pub nullable: bool,
    pub ordinal_position: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: usize,
    pub truncated: bool,
    /// One entry per column. `Some` holds the enum labels for that column.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enum_values: Vec<Option<Vec<String>>>,
}

/// Community `FieldDescriptor` (`apps/studio/src/lib/db/models.ts`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FieldDescriptor {
    pub name: String,
    pub id: String,
    pub data_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

/// Community `NgQueryResult`. Rows are objects keyed by field id.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NgQueryResult {
    pub fields: Vec<FieldDescriptor>,
    pub rows: Vec<serde_json::Value>,
    pub row_count: usize,
    pub total_row_count: Option<usize>,
    pub truncated: bool,
    pub command: Option<String>,
    pub affected_rows: Option<u64>,
    pub text: Option<String>,
}

impl From<QueryResult> for NgQueryResult {
    fn from(result: QueryResult) -> Self {
        tabular_to_ng(
            result.columns,
            result.rows,
            result.row_count,
            result.truncated,
            None,
            result.enum_values,
        )
    }
}

impl From<TableResult> for NgQueryResult {
    fn from(result: TableResult) -> Self {
        tabular_to_ng(
            result.columns,
            result.rows,
            result.total.max(0) as usize,
            false,
            Some(result.total.max(0) as usize),
            result.enum_values,
        )
    }
}

impl From<NgQueryResult> for QueryResult {
    fn from(result: NgQueryResult) -> Self {
        let columns: Vec<String> = result.fields.iter().map(|field| field.name.clone()).collect();
        let rows = result
            .rows
            .into_iter()
            .map(|row| match row {
                serde_json::Value::Object(map) => result
                    .fields
                    .iter()
                    .map(|field| map.get(&field.id).cloned().unwrap_or(serde_json::Value::Null))
                    .collect(),
                serde_json::Value::Array(cells) => cells,
                other => vec![other],
            })
            .collect();
        let enum_values = result
            .fields
            .iter()
            .map(|field| field.enum_values.clone())
            .collect();
        QueryResult {
            columns,
            rows,
            row_count: result.row_count,
            truncated: result.truncated,
            enum_values,
        }
    }
}

fn tabular_to_ng(
    columns: Vec<String>,
    rows: Vec<Vec<serde_json::Value>>,
    row_count: usize,
    truncated: bool,
    total_row_count: Option<usize>,
    enum_values: Vec<Option<Vec<String>>>,
) -> NgQueryResult {
    let fields: Vec<FieldDescriptor> = columns
        .iter()
        .enumerate()
        .map(|(index, name)| FieldDescriptor {
            name: name.clone(),
            id: name.clone(),
            data_type: None,
            enum_values: enum_values.get(index).cloned().flatten(),
        })
        .collect();
    let objects = rows
        .into_iter()
        .map(|row| {
            let mut map = serde_json::Map::new();
            for (index, field) in fields.iter().enumerate() {
                map.insert(
                    field.id.clone(),
                    row.get(index).cloned().unwrap_or(serde_json::Value::Null),
                );
            }
            serde_json::Value::Object(map)
        })
        .collect();
    NgQueryResult {
        fields,
        rows: objects,
        row_count,
        total_row_count,
        truncated,
        command: Some("SELECT".into()),
        affected_rows: None,
        text: None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TableIndex {
    pub name: String,
    pub unique: bool,
    pub primary: bool,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TableTrigger {
    pub name: String,
    pub timing: Option<String>,
    pub manipulation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Routine {
    pub name: String,
    pub schema: Option<String>,
    pub routine_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SelectTop {
    pub table: String,
    pub schema: Option<String>,
    pub offset: i64,
    pub limit: i64,
    pub order_by: Vec<OrderBy>,
    pub filters: Vec<crate::filter::TableFilter>,
    pub selects: Option<Vec<String>>,
    #[serde(default)]
    pub skip_count: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderBy {
    pub field: String,
    pub dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TableResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub total: i64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enum_values: Vec<Option<Vec<String>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TableChanges {
    pub inserts: Vec<RowChange>,
    pub updates: Vec<RowChange>,
    pub deletes: Vec<RowChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RowChange {
    pub table: String,
    pub schema: Option<String>,
    pub primary_keys: Vec<(String, serde_json::Value)>,
    pub values: Vec<(String, serde_json::Value)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SupportedFeatures {
    pub custom_filter: bool,
    pub editable: bool,
    pub json_viewer: bool,
    pub import: bool,
    pub export: bool,
    pub backup: bool,
}

impl SupportedFeatures {
    pub fn all() -> Self {
        Self {
            custom_filter: true,
            editable: true,
            json_viewer: true,
            import: true,
            export: true,
            backup: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelableQuery {
    pub id: u32,
    pub name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn query_result_becomes_community_ng_objects() {
        let raw = QueryResult {
            columns: vec!["id".into(), "name".into(), "status".into()],
            rows: vec![vec![json!(1), json!("a"), json!("open")]],
            row_count: 1,
            truncated: false,
            enum_values: vec![None, None, Some(vec!["open".into(), "closed".into()])],
        };
        let ng = NgQueryResult::from(raw);
        assert_eq!(ng.fields[0].id, "id");
        assert_eq!(ng.rows[0]["name"], json!("a"));
        assert_eq!(
            ng.fields[2].enum_values.as_deref(),
            Some(["open".to_string(), "closed".to_string()].as_slice())
        );
        assert_eq!(ng.command.as_deref(), Some("SELECT"));
        let back = QueryResult::from(ng);
        assert_eq!(back.columns, vec!["id", "name", "status"]);
        assert_eq!(back.rows[0][1], json!("a"));
        assert_eq!(
            back.enum_values[2].as_deref(),
            Some(["open".to_string(), "closed".to_string()].as_slice())
        );
    }
}
