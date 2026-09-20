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
#[serde(rename_all = "camelCase")]
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
