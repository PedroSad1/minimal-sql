use async_trait::async_trait;
use graphite_core::{GraphiteError, Result};
use graphite_core::{
    ConnectionConfig, DatabaseClient, QueryResult, SelectTop, TableChanges, TableColumn,
    TableOrView, TableResult,
};
use serde_json::{json, Value};

pub struct BigQueryClient {
    config: ConnectionConfig,
    token: Option<String>,
    http: reqwest::Client,
}

impl BigQueryClient {
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            config,
            token: None,
            http: reqwest::Client::new(),
        }
    }

    fn project(&self) -> Result<String> {
        self.config
            .project_id
            .clone()
            .ok_or_else(|| GraphiteError::msg("BigQuery requires projectId"))
    }

    async fn bearer(&self) -> Result<String> {
        self.token
            .clone()
            .ok_or(GraphiteError::NotConnected)
    }
}

#[async_trait]
impl DatabaseClient for BigQueryClient {
    async fn connect(&mut self) -> Result<()> {
        let token = fetch_token(&self.config).await?;
        self.token = Some(token);
        let _ = self.project()?;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.token = None;
        Ok(())
    }

    async fn version_string(&self) -> Result<String> {
        Ok(format!("BigQuery {}", self.project()?))
    }

    async fn list_tables(&self) -> Result<Vec<TableOrView>> {
        let project = self.project()?;
        let dataset = self
            .config
            .dataset
            .clone()
            .ok_or_else(|| GraphiteError::msg("BigQuery requires dataset"))?;
        let token = self.bearer().await?;
        let url = format!(
            "https://bigquery.googleapis.com/bigquery/v2/projects/{project}/datasets/{dataset}/tables"
        );
        let body: Value = self
            .http
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?
            .json()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let tables = body
            .get("tables")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(tables
            .into_iter()
            .filter_map(|t| {
                let name = t
                    .pointer("/tableReference/tableId")?
                    .as_str()?
                    .to_string();
                Some(TableOrView {
                    name,
                    schema: Some(dataset.clone()),
                    entity_type: "table".into(),
                    parent: None,
                })
            })
            .collect())
    }

    async fn list_views(&self) -> Result<Vec<TableOrView>> {
        Ok(vec![])
    }

    async fn list_table_columns(&self, table: &str, schema: Option<&str>) -> Result<Vec<TableColumn>> {
        let project = self.project()?;
        let dataset = schema
            .map(|s| s.to_string())
            .or_else(|| self.config.dataset.clone())
            .ok_or_else(|| GraphiteError::msg("dataset required"))?;
        let token = self.bearer().await?;
        let url = format!(
            "https://bigquery.googleapis.com/bigquery/v2/projects/{project}/datasets/{dataset}/tables/{table}"
        );
        let body: Value = self
            .http
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?
            .json()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let fields = body
            .pointer("/schema/fields")
            .and_then(|f| f.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(fields
            .into_iter()
            .enumerate()
            .map(|(i, f)| TableColumn {
                column_name: f
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("col")
                    .into(),
                data_type: f
                    .get("type")
                    .and_then(|n| n.as_str())
                    .unwrap_or("STRING")
                    .into(),
                nullable: f.get("mode").and_then(|m| m.as_str()) != Some("REQUIRED"),
                ordinal_position: (i as i32) + 1,
            })
            .collect())
    }

    async fn execute_query(&self, sql: &str) -> Result<Vec<QueryResult>> {
        let project = self.project()?;
        let token = self.bearer().await?;
        let url = format!(
            "https://bigquery.googleapis.com/bigquery/v2/projects/{project}/queries"
        );
        let body = json!({
            "query": sql,
            "useLegacySql": false,
            "maxResults": 5000
        });
        let resp: Value = self
            .http
            .post(url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?
            .json()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        if let Some(err) = resp.pointer("/error/message").and_then(|m| m.as_str()) {
            return Err(GraphiteError::msg(err.to_string()));
        }
        Ok(vec![bq_to_result(&resp)])
    }

    async fn select_top(&self, opts: SelectTop) -> Result<TableResult> {
        let dataset = opts
            .schema
            .clone()
            .or_else(|| self.config.dataset.clone())
            .unwrap_or_default();
        let table = if dataset.is_empty() {
            format!("`{}`", opts.table)
        } else {
            format!("`{dataset}.{}`", opts.table)
        };
        let sql = format!("SELECT * FROM {table} LIMIT {} OFFSET {}", opts.limit.max(1), opts.offset.max(0));
        let result = self.execute_query(&sql).await?.into_iter().next().unwrap_or_default();
        Ok(TableResult {
            total: result.row_count as i64,
            columns: result.columns,
            rows: result.rows,
        })
    }

    async fn apply_changes(&self, _changes: TableChanges) -> Result<u64> {
        Err(GraphiteError::msg("BigQuery apply_changes is not enabled"))
    }

    async fn get_primary_keys(&self, _table: &str, _schema: Option<&str>) -> Result<Vec<String>> {
        Ok(vec![])
    }

    async fn list_databases(&self) -> Result<Vec<String>> {
        let project = self.project()?;
        let token = self.bearer().await?;
        let url = format!(
            "https://bigquery.googleapis.com/bigquery/v2/projects/{project}/datasets"
        );
        let body: Value = self
            .http
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?
            .json()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(body
            .get("datasets")
            .and_then(|d| d.as_array())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|d| {
                d.pointer("/datasetReference/datasetId")?
                    .as_str()
                    .map(|s| s.to_string())
            })
            .collect())
    }
}

fn bq_to_result(body: &Value) -> QueryResult {
    let columns = body
        .pointer("/schema/fields")
        .and_then(|f| f.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|f| f.get("name")?.as_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();
    let rows = body
        .get("rows")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|row| {
            row.get("f")
                .and_then(|f| f.as_array())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|cell| cell.get("v").cloned().unwrap_or(Value::Null))
                .collect()
        })
        .collect::<Vec<_>>();
    QueryResult {
        row_count: rows.len(),
        truncated: false,
        columns,
        rows,
    }
}

async fn fetch_token(config: &ConnectionConfig) -> Result<String> {
    if let Ok(token) = std::env::var("GOOGLE_OAUTH_ACCESS_TOKEN") {
        return Ok(token);
    }
    let path = config
        .service_account_json
        .clone()
        .or_else(|| std::env::var("GOOGLE_APPLICATION_CREDENTIALS").ok())
        .ok_or_else(|| {
            GraphiteError::msg(
                "BigQuery needs serviceAccountJson or GOOGLE_APPLICATION_CREDENTIALS",
            )
        })?;
    let json: Value = serde_json::from_str(
        &std::fs::read_to_string(&path).map_err(|e| GraphiteError::msg(e.to_string()))?,
    )
    .map_err(|e| GraphiteError::msg(e.to_string()))?;
    let client_email = json
        .get("client_email")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GraphiteError::msg("service account missing client_email"))?;
    let private_key = json
        .get("private_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| GraphiteError::msg("service account missing private_key"))?;
    let jwt = encode_jwt(client_email, private_key)?;
    let http = reqwest::Client::new();
    let resp: Value = http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
        ])
        .send()
        .await
        .map_err(|e| GraphiteError::msg(e.to_string()))?
        .json()
        .await
        .map_err(|e| GraphiteError::msg(e.to_string()))?;
    resp.get("access_token")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| GraphiteError::msg(format!("token error: {resp}")))
}

fn encode_jwt(client_email: &str, private_key: &str) -> Result<String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde::Serialize;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Serialize)]
    struct Claims {
        iss: String,
        scope: String,
        aud: String,
        exp: u64,
        iat: u64,
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| GraphiteError::msg(e.to_string()))?
        .as_secs();
    let claims = Claims {
        iss: client_email.to_string(),
        scope: "https://www.googleapis.com/auth/bigquery".into(),
        aud: "https://oauth2.googleapis.com/token".into(),
        iat: now,
        exp: now + 3600,
    };
    let key = EncodingKey::from_rsa_pem(private_key.as_bytes())
        .map_err(|e| GraphiteError::msg(e.to_string()))?;
    encode(&Header::new(Algorithm::RS256), &claims, &key)
        .map_err(|e| GraphiteError::msg(e.to_string()))
}
