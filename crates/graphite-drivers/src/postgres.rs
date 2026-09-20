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
use tokio_postgres::{NoTls, Row};

use crate::params::SqlVal;
use crate::sqlutil::{limit_sql, qualify};

pub struct PostgresClient {
    config: ConnectionConfig,
    client: Option<Arc<tokio_postgres::Client>>,
}

impl PostgresClient {
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            config,
            client: None,
        }
    }

    fn url(&self) -> String {
        let host = self.config.host.clone().unwrap_or_else(|| "127.0.0.1".into());
        let port = self.config.port.unwrap_or(5432);
        let user = self.config.user.clone().unwrap_or_else(|| "postgres".into());
        let pass = self.config.password.clone().unwrap_or_default();
        let db = self
            .config
            .default_database
            .clone()
            .unwrap_or_else(|| "postgres".into());
        format!("host={host} port={port} user={user} password={pass} dbname={db}")
    }

    fn client(&self) -> Result<Arc<tokio_postgres::Client>> {
        self.client.clone().ok_or(GraphiteError::NotConnected)
    }
}

#[async_trait]
impl DatabaseClient for PostgresClient {
    async fn connect(&mut self) -> Result<()> {
        let (client, connection) = tokio_postgres::connect(&self.url(), NoTls)
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        self.client = Some(Arc::new(client));
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.client = None;
        Ok(())
    }

    async fn version_string(&self) -> Result<String> {
        let client = self.client()?;
        let row = client
            .query_one("select version()", &[])
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(row.get::<_, String>(0))
    }

    async fn list_tables(&self) -> Result<Vec<TableOrView>> {
        self.list_rel("BASE TABLE", "table").await
    }

    async fn list_views(&self) -> Result<Vec<TableOrView>> {
        self.list_rel("VIEW", "view").await
    }

    async fn list_table_columns(&self, table: &str, schema: Option<&str>) -> Result<Vec<TableColumn>> {
        let client = self.client()?;
        let schema = schema.unwrap_or("public");
        let rows = client
            .query(
                "select column_name, data_type, is_nullable, ordinal_position
                 from information_schema.columns
                 where table_name = $1 and table_schema = $2
                 order by ordinal_position",
                &[&table, &schema],
            )
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|row| TableColumn {
                column_name: row.get(0),
                data_type: row.get(1),
                nullable: row.get::<_, String>(2) == "YES",
                ordinal_position: row.get::<_, i32>(3),
            })
            .collect())
    }

    async fn execute_query(&self, sql: &str) -> Result<Vec<QueryResult>> {
        let client = self.client()?;
        query_pg(&client, sql, &[]).await
    }

    async fn select_top(&self, opts: SelectTop) -> Result<TableResult> {
        let dialect = Dialect::Postgres;
        let table = qualify(dialect, opts.schema.as_deref().or(Some("public")), &opts.table)?;
        let where_clause = build_where(dialect, &opts.filters)?;
        let order = order_sql(dialect, &opts)?;
        let cols = select_sql(dialect, &opts)?;
        let sql = format!(
            "SELECT {cols} FROM {table}{}{order}{}",
            where_clause.sql,
            limit_sql(dialect, opts.limit.max(1), opts.offset.max(0))
        );
        let count_sql = format!("SELECT COUNT(*) FROM {table}{}", where_clause.sql);
        let client = self.client()?;
        let params: Vec<SqlVal> = where_clause.params.iter().map(SqlVal::from).collect();
        let result = query_pg(&client, &sql, &params).await?.into_iter().next().unwrap_or_default();
        let count_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params.iter().map(|p| p as _).collect();
        let total = client
            .query_one(&count_sql, &count_params)
            .await
            .ok()
            .and_then(|row| row.try_get::<_, i64>(0).ok())
            .unwrap_or(result.row_count as i64);
        Ok(TableResult {
            columns: result.columns,
            rows: result.rows,
            total,
        })
    }

    async fn apply_changes(&self, changes: TableChanges) -> Result<u64> {
        let client = self.client()?;
        let dialect = Dialect::Postgres;
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
            let params: Vec<SqlVal> = insert.values.iter().map(|(_, v)| SqlVal::from(v)).collect();
            let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                params.iter().map(|p| p as _).collect();
            affected += client
                .execute(&sql, &refs)
                .await
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
        }
        for update in &changes.updates {
            let table = qualify(dialect, update.schema.as_deref(), &update.table)?;
            let mut params = Vec::new();
            let mut sets = Vec::new();
            for (col, val) in &update.values {
                sets.push(format!("{} = {}", dialect.quote_ident(col)?, dialect.placeholder(params.len())));
                params.push(SqlVal::from(val));
            }
            let mut wheres = Vec::new();
            for (col, val) in &update.primary_keys {
                wheres.push(format!("{} = {}", dialect.quote_ident(col)?, dialect.placeholder(params.len())));
                params.push(SqlVal::from(val));
            }
            let sql = format!("UPDATE {table} SET {} WHERE {}", sets.join(", "), wheres.join(" AND "));
            let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                params.iter().map(|p| p as _).collect();
            affected += client
                .execute(&sql, &refs)
                .await
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
        }
        for delete in &changes.deletes {
            let table = qualify(dialect, delete.schema.as_deref(), &delete.table)?;
            let mut params = Vec::new();
            let mut wheres = Vec::new();
            for (col, val) in &delete.primary_keys {
                wheres.push(format!("{} = {}", dialect.quote_ident(col)?, dialect.placeholder(params.len())));
                params.push(SqlVal::from(val));
            }
            let sql = format!("DELETE FROM {table} WHERE {}", wheres.join(" AND "));
            let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                params.iter().map(|p| p as _).collect();
            affected += client
                .execute(&sql, &refs)
                .await
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
        }
        Ok(affected)
    }

    async fn get_primary_keys(&self, table: &str, schema: Option<&str>) -> Result<Vec<String>> {
        let client = self.client()?;
        let schema = schema.unwrap_or("public");
        let rows = client
            .query(
                "select kcu.column_name
                 from information_schema.table_constraints tc
                 join information_schema.key_column_usage kcu
                   on tc.constraint_name = kcu.constraint_name
                  and tc.table_schema = kcu.table_schema
                 where tc.constraint_type = 'PRIMARY KEY'
                   and tc.table_name = $1 and tc.table_schema = $2",
                &[&table, &schema],
            )
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(rows.into_iter().map(|r| r.get(0)).collect())
    }

    async fn list_databases(&self) -> Result<Vec<String>> {
        let client = self.client()?;
        let rows = client
            .query("select datname from pg_database where datistemplate = false", &[])
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(rows.into_iter().map(|r| r.get(0)).collect())
    }

    async fn default_schema(&self) -> Result<Option<String>> {
        Ok(Some("public".into()))
    }
}

impl PostgresClient {
    async fn list_rel(&self, table_type: &str, entity: &str) -> Result<Vec<TableOrView>> {
        let client = self.client()?;
        let rows = client
            .query(
                "select table_schema, table_name from information_schema.tables
                 where table_type = $1 and table_schema not in ('pg_catalog','information_schema')
                 order by table_schema, table_name",
                &[&table_type],
            )
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|row| TableOrView {
                schema: Some(row.get(0)),
                name: row.get(1),
                entity_type: entity.into(),
            })
            .collect())
    }
}

pub(crate) fn select_sql(dialect: Dialect, opts: &SelectTop) -> Result<String> {
    if let Some(selects) = &opts.selects {
        let quoted: Result<Vec<String>> = selects.iter().map(|c| dialect.quote_ident(c)).collect();
        Ok(quoted?.join(", "))
    } else {
        Ok("*".into())
    }
}

pub(crate) fn order_sql(dialect: Dialect, opts: &SelectTop) -> Result<String> {
    if opts.order_by.is_empty() {
        return Ok(String::new());
    }
    let bits: Result<Vec<String>> = opts
        .order_by
        .iter()
        .map(|o| {
            let ident = dialect.quote_ident(&o.field)?;
            let dir = if o.dir.eq_ignore_ascii_case("desc") {
                "DESC"
            } else {
                "ASC"
            };
            Ok(format!("{ident} {dir}"))
        })
        .collect();
    Ok(format!(" ORDER BY {}", bits?.join(", ")))
}

async fn query_pg(
    client: &tokio_postgres::Client,
    sql: &str,
    params: &[SqlVal],
) -> Result<Vec<QueryResult>> {
    let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
        params.iter().map(|p| p as _).collect();
    match client.query(sql, &refs).await {
        Ok(rows) => Ok(vec![rows_to_result(&rows)]),
        Err(_) => {
            let n = client
                .execute(sql, &refs)
                .await
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            Ok(vec![QueryResult {
                columns: vec!["rows".into()],
                rows: vec![vec![json!(n)]],
                row_count: 1,
                truncated: false,
            }])
        }
    }
}

fn rows_to_result(rows: &[Row]) -> QueryResult {
    let columns = if let Some(row) = rows.first() {
        row.columns().iter().map(|c| c.name().to_string()).collect()
    } else {
        vec![]
    };
    let mut out = Vec::new();
    for row in rows.iter().take(5000) {
        let mut values = Vec::new();
        for i in 0..row.len() {
            values.push(pg_cell(row, i));
        }
        out.push(values);
    }
    QueryResult {
        row_count: out.len(),
        truncated: rows.len() > 5000,
        columns,
        rows: out,
    }
}

fn pg_cell(row: &Row, i: usize) -> Value {
    if let Ok(v) = row.try_get::<_, Option<i64>>(i) {
        return json!(v);
    }
    if let Ok(v) = row.try_get::<_, Option<f64>>(i) {
        return json!(v);
    }
    if let Ok(v) = row.try_get::<_, Option<bool>>(i) {
        return json!(v);
    }
    if let Ok(v) = row.try_get::<_, Option<String>>(i) {
        return json!(v);
    }
    Value::Null
}
