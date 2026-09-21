use std::sync::Arc;

use async_trait::async_trait;
use graphite_core::Dialect;
use graphite_core::{GraphiteError, Result};
use graphite_core::build_where;
use graphite_core::{
    ConnectionConfig, DatabaseClient, QueryResult, SelectTop, TableChanges, TableColumn,
    TableOrView, TableResult,
};
use serde_json::{json, Value};
use tiberius::{AuthMethod, Client, Config, Query};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use crate::postgres::{order_sql, select_sql};
use crate::sqlutil::{limit_sql, qualify};

type MssqlStream = Client<Compat<TcpStream>>;

pub struct MssqlClient {
    config: ConnectionConfig,
    client: Option<Arc<Mutex<MssqlStream>>>,
}

impl MssqlClient {
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            config,
            client: None,
        }
    }

    fn handle(&self) -> Result<Arc<Mutex<MssqlStream>>> {
        self.client.clone().ok_or(GraphiteError::NotConnected)
    }
}

#[async_trait]
impl DatabaseClient for MssqlClient {
    async fn connect(&mut self) -> Result<()> {
        let mut cfg = Config::new();
        let host = self.config.host.clone().unwrap_or_else(|| "127.0.0.1".into());
        let port = self.config.port.unwrap_or(1433);
        cfg.host(host.clone());
        cfg.port(port);
        cfg.database(
            self.config
                .default_database
                .clone()
                .unwrap_or_else(|| "master".into()),
        );
        cfg.authentication(AuthMethod::sql_server(
            self.config.user.clone().unwrap_or_else(|| "sa".into()),
            self.config.password.clone().unwrap_or_default(),
        ));
        cfg.trust_cert();
        let tcp = TcpStream::connect(format!("{host}:{port}"))
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        tcp.set_nodelay(true)
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let client = Client::connect(cfg, tcp.compat_write())
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        self.client = Some(Arc::new(Mutex::new(client)));
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.client = None;
        Ok(())
    }

    async fn version_string(&self) -> Result<String> {
        let rows = self.query_sql("select @@version", &[]).await?;
        Ok(rows
            .rows
            .first()
            .and_then(|r| r.first())
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_else(|| "SQL Server".into()))
    }

    async fn list_tables(&self) -> Result<Vec<TableOrView>> {
        self.list_kind("U", "table").await
    }

    async fn list_views(&self) -> Result<Vec<TableOrView>> {
        self.list_kind("V", "view").await
    }

    async fn list_table_columns(&self, table: &str, schema: Option<&str>) -> Result<Vec<TableColumn>> {
        let schema = schema.unwrap_or("dbo");
        let result = self
            .query_sql(
                "select c.name, ty.name, c.is_nullable, c.column_id
                 from sys.columns c
                 join sys.types ty on c.user_type_id = ty.user_type_id
                 join sys.tables tb on c.object_id = tb.object_id
                 join sys.schemas s on tb.schema_id = s.schema_id
                 where tb.name = @P1 and s.name = @P2
                 order by c.column_id",
                &[json!(table), json!(schema)],
            )
            .await?;
        Ok(result
            .rows
            .into_iter()
            .map(|row| TableColumn {
                column_name: cell_string(&row, 0),
                data_type: cell_string(&row, 1),
                nullable: row.get(2).and_then(|v| v.as_bool()).unwrap_or(true),
                ordinal_position: row.get(3).and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            })
            .collect())
    }

    async fn execute_query(&self, sql: &str) -> Result<Vec<QueryResult>> {
        Ok(vec![self.query_sql(sql, &[]).await?])
    }

    async fn select_top(&self, opts: SelectTop) -> Result<TableResult> {
        let dialect = Dialect::Sqlserver;
        let table = qualify(dialect, opts.schema.as_deref().or(Some("dbo")), &opts.table)?;
        let where_clause = build_where(dialect, &opts.filters)?;
        let order = if opts.order_by.is_empty() {
            " ORDER BY (SELECT NULL)".into()
        } else {
            order_sql(dialect, &opts)?
        };
        let sql = format!(
            "SELECT {} FROM {table}{}{order}{}",
            select_sql(dialect, &opts)?,
            where_clause.sql,
            limit_sql(dialect, opts.limit.max(1), opts.offset.max(0))
        );
        let result = self.query_sql(&sql, &where_clause.params).await?;
        Ok(TableResult {
            total: result.row_count as i64,
            columns: result.columns,
            rows: result.rows,
        })
    }

    async fn apply_changes(&self, changes: TableChanges) -> Result<u64> {
        let dialect = Dialect::Sqlserver;
        let mut affected = 0u64;
        for insert in &changes.inserts {
            let table = qualify(dialect, insert.schema.as_deref(), &insert.table)?;
            let cols: Result<Vec<String>> = insert
                .values
                .iter()
                .map(|(c, _)| dialect.quote_ident(c))
                .collect();
            let cols = cols?;
            let placeholders: Vec<String> = (0..cols.len()).map(|i| dialect.placeholder(i)).collect();
            let sql = format!(
                "INSERT INTO {table} ({}) VALUES ({})",
                cols.join(", "),
                placeholders.join(", ")
            );
            let params: Vec<Value> = insert.values.iter().map(|(_, v)| v.clone()).collect();
            self.query_sql(&sql, &params).await?;
            affected += 1;
        }
        Ok(affected)
    }

    async fn get_primary_keys(&self, table: &str, schema: Option<&str>) -> Result<Vec<String>> {
        let schema = schema.unwrap_or("dbo");
        let result = self
            .query_sql(
                "select c.name
                 from sys.indexes i
                 join sys.index_columns ic on i.object_id = ic.object_id and i.index_id = ic.index_id
                 join sys.columns c on ic.object_id = c.object_id and ic.column_id = c.column_id
                 join sys.tables t on i.object_id = t.object_id
                 join sys.schemas s on t.schema_id = s.schema_id
                 where i.is_primary_key = 1 and t.name = @P1 and s.name = @P2",
                &[json!(table), json!(schema)],
            )
            .await?;
        Ok(result.rows.into_iter().map(|r| cell_string(&r, 0)).collect())
    }

    async fn list_databases(&self) -> Result<Vec<String>> {
        let result = self.query_sql("select name from sys.databases", &[]).await?;
        Ok(result.rows.into_iter().map(|r| cell_string(&r, 0)).collect())
    }
}

impl MssqlClient {
    async fn list_kind(&self, ty: &str, entity: &str) -> Result<Vec<TableOrView>> {
        let result = self
            .query_sql(
                "select s.name, t.name from sys.objects t
                 join sys.schemas s on t.schema_id = s.schema_id
                 where t.type = @P1 order by s.name, t.name",
                &[json!(ty)],
            )
            .await?;
        Ok(result
            .rows
            .into_iter()
            .map(|row| TableOrView {
                schema: Some(cell_string(&row, 0)),
                name: cell_string(&row, 1),
                entity_type: entity.into(),
                parent: None,
            })
            .collect())
    }

    async fn query_sql(&self, sql: &str, params: &[Value]) -> Result<QueryResult> {
        let handle = self.handle()?;
        let mut client = handle.lock().await;
        let mut query = Query::new(sql);
        for param in params {
            bind_mssql(&mut query, param);
        }
        let stream = query
            .query(&mut *client)
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let rows = stream
            .into_first_result()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(mssql_result(rows))
    }
}

fn bind_mssql(query: &mut Query<'_>, value: &Value) {
    match value {
        Value::Null => {
            query.bind(Option::<String>::None);
        }
        Value::Bool(v) => {
            query.bind(*v);
        }
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                query.bind(i);
            } else if let Some(f) = n.as_f64() {
                query.bind(f);
            } else {
                query.bind(n.to_string());
            }
        }
        Value::String(text) => {
            query.bind(text.clone());
        }
        other => {
            query.bind(other.to_string());
        }
    }
}

fn mssql_result(rows: Vec<tiberius::Row>) -> QueryResult {
    let columns = rows
        .first()
        .map(|row| row.columns().iter().map(|c| c.name().to_string()).collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    for row in rows.into_iter().take(5000) {
        let mut values = Vec::new();
        for i in 0..row.columns().len() {
            values.push(mssql_cell(&row, i));
        }
        out.push(values);
    }
    QueryResult {
        row_count: out.len(),
        truncated: false,
        columns,
        rows: out,
    }
}

fn mssql_cell(row: &tiberius::Row, i: usize) -> Value {
    if let Ok(Some(v)) = row.try_get::<i64, _>(i) {
        return json!(v);
    }
    if let Ok(Some(v)) = row.try_get::<i32, _>(i) {
        return json!(v);
    }
    if let Ok(Some(v)) = row.try_get::<f64, _>(i) {
        return json!(v);
    }
    if let Ok(Some(v)) = row.try_get::<bool, _>(i) {
        return json!(v);
    }
    if let Ok(Some(v)) = row.try_get::<&str, _>(i) {
        return json!(v);
    }
    Value::Null
}

fn cell_string(row: &[Value], i: usize) -> String {
    match row.get(i) {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string().trim_matches('"').to_string(),
        None => String::new(),
    }
}
