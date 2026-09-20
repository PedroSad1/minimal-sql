use std::sync::Arc;

use async_trait::async_trait;
use graphite_core::{GraphiteError, Result};
use graphite_core::{
    ConnectionConfig, DatabaseClient, QueryResult, SelectTop, TableChanges, TableColumn,
    TableOrView, TableResult,
};
use redis::AsyncCommands;
use serde_json::{json, Value};
use tokio::sync::Mutex;

pub struct RedisClient {
    config: ConnectionConfig,
    conn: Option<Arc<Mutex<redis::aio::MultiplexedConnection>>>,
}

impl RedisClient {
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            config,
            conn: None,
        }
    }

    fn conn(&self) -> Result<Arc<Mutex<redis::aio::MultiplexedConnection>>> {
        self.conn.clone().ok_or(GraphiteError::NotConnected)
    }
}

#[async_trait]
impl DatabaseClient for RedisClient {
    async fn connect(&mut self) -> Result<()> {
        let host = self.config.host.clone().unwrap_or_else(|| "127.0.0.1".into());
        let port = self.config.port.unwrap_or(6379);
        let pass = self.config.password.clone().unwrap_or_default();
        let db = self
            .config
            .default_database
            .clone()
            .unwrap_or_else(|| "0".into());
        let url = if pass.is_empty() {
            format!("redis://{host}:{port}/{db}")
        } else {
            format!("redis://:{pass}@{host}:{port}/{db}")
        };
        let client = redis::Client::open(url).map_err(|e| GraphiteError::msg(e.to_string()))?;
        let conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        self.conn = Some(Arc::new(Mutex::new(conn)));
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.conn = None;
        Ok(())
    }

    async fn version_string(&self) -> Result<String> {
        let conn = self.conn()?;
        let mut conn = conn.lock().await;
        let info: String = redis::cmd("INFO")
            .arg("server")
            .query_async(&mut *conn)
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(info
            .lines()
            .find(|l| l.starts_with("redis_version"))
            .unwrap_or("redis")
            .to_string())
    }

    async fn list_tables(&self) -> Result<Vec<TableOrView>> {
        let conn = self.conn()?;
        let mut conn = conn.lock().await;
        let keys: Vec<String> = conn
            .keys("*")
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(keys
            .into_iter()
            .map(|name| TableOrView {
                name,
                schema: None,
                entity_type: "key".into(),
            })
            .collect())
    }

    async fn list_views(&self) -> Result<Vec<TableOrView>> {
        Ok(vec![])
    }

    async fn list_table_columns(&self, _table: &str, _schema: Option<&str>) -> Result<Vec<TableColumn>> {
        Ok(vec![
            TableColumn {
                column_name: "field".into(),
                data_type: "string".into(),
                nullable: true,
                ordinal_position: 1,
            },
            TableColumn {
                column_name: "value".into(),
                data_type: "string".into(),
                nullable: true,
                ordinal_position: 2,
            },
        ])
    }

    async fn execute_query(&self, sql: &str) -> Result<Vec<QueryResult>> {
        let conn = self.conn()?;
        let mut conn = conn.lock().await;
        let parts: Vec<&str> = sql.split_whitespace().collect();
        if parts.is_empty() {
            return Err(GraphiteError::msg("empty redis command"));
        }
        let mut cmd = redis::cmd(parts[0]);
        for arg in &parts[1..] {
            cmd.arg(*arg);
        }
        let value: redis::Value = cmd
            .query_async(&mut *conn)
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(vec![redis_to_result(value)])
    }

    async fn select_top(&self, opts: SelectTop) -> Result<TableResult> {
        let conn = self.conn()?;
        let mut conn = conn.lock().await;
        let key_type: String = conn
            .key_type(&opts.table)
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let mut rows = Vec::new();
        match key_type.as_str() {
            "string" => {
                let v: String = conn
                    .get(&opts.table)
                    .await
                    .map_err(|e| GraphiteError::msg(e.to_string()))?;
                rows.push(vec![json!("value"), json!(v)]);
            }
            "hash" => {
                let map: Vec<(String, String)> = conn
                    .hgetall(&opts.table)
                    .await
                    .map_err(|e| GraphiteError::msg(e.to_string()))?;
                for (k, v) in map {
                    rows.push(vec![json!(k), json!(v)]);
                }
            }
            "list" => {
                let values: Vec<String> = conn
                    .lrange(&opts.table, opts.offset as isize, (opts.offset + opts.limit - 1) as isize)
                    .await
                    .map_err(|e| GraphiteError::msg(e.to_string()))?;
                for (i, v) in values.into_iter().enumerate() {
                    rows.push(vec![json!(i as i64), json!(v)]);
                }
            }
            other => rows.push(vec![json!("type"), json!(other)]),
        }
        let total = rows.len() as i64;
        Ok(TableResult {
            columns: vec!["field".into(), "value".into()],
            rows,
            total,
        })
    }

    async fn apply_changes(&self, changes: TableChanges) -> Result<u64> {
        let conn = self.conn()?;
        let mut conn = conn.lock().await;
        let mut n = 0u64;
        for insert in &changes.inserts {
            if let (Some((_, key)), Some((_, val))) =
                (insert.values.first(), insert.values.get(1))
            {
                let _: () = conn
                    .set(json_str(key), json_str(val))
                    .await
                    .map_err(|e| GraphiteError::msg(e.to_string()))?;
                n += 1;
            }
        }
        Ok(n)
    }

    async fn get_primary_keys(&self, _table: &str, _schema: Option<&str>) -> Result<Vec<String>> {
        Ok(vec!["field".into()])
    }

    async fn list_databases(&self) -> Result<Vec<String>> {
        Ok((0..16).map(|i| i.to_string()).collect())
    }
}

fn json_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn redis_to_result(value: redis::Value) -> QueryResult {
    let rows = flatten_redis(value)
        .into_iter()
        .map(|v| vec![json!(v)])
        .collect::<Vec<_>>();
    QueryResult {
        row_count: rows.len(),
        truncated: false,
        columns: vec!["value".into()],
        rows,
    }
}

fn flatten_redis(value: redis::Value) -> Vec<String> {
    match value {
        redis::Value::Nil => vec!["null".into()],
        redis::Value::Int(n) => vec![n.to_string()],
        redis::Value::BulkString(bytes) => {
            vec![String::from_utf8_lossy(&bytes).to_string()]
        }
        redis::Value::SimpleString(s) => vec![s],
        redis::Value::Array(items) => items.into_iter().flat_map(flatten_redis).collect(),
        other => vec![format!("{other:?}")],
    }
}
