use async_trait::async_trait;
use graphite_core::Dialect;
use graphite_core::{GraphiteError, Result};
use graphite_core::build_where;
use graphite_core::{
    ConnectionConfig, DatabaseClient, QueryResult, SelectTop, TableChanges, TableColumn,
    TableOrView, TableResult,
};
use mysql_async::prelude::*;
use mysql_async::{OptsBuilder, Pool, Row as MysqlRow};
use serde_json::json;

use crate::params::{json_to_mysql, mysql_to_json};
use crate::postgres::{order_sql, select_sql};
use crate::sqlutil::{limit_sql, qualify};

pub struct MysqlClient {
    config: ConnectionConfig,
    pool: Option<Pool>,
}

impl MysqlClient {
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            config,
            pool: None,
        }
    }

    fn pool(&self) -> Result<&Pool> {
        self.pool.as_ref().ok_or(GraphiteError::NotConnected)
    }
}

#[async_trait]
impl DatabaseClient for MysqlClient {
    async fn connect(&mut self) -> Result<()> {
        let host = self.config.host.clone().unwrap_or_else(|| "127.0.0.1".into());
        let port = self.config.port.unwrap_or(3306);
        let user = self.config.user.clone().unwrap_or_else(|| "root".into());
        let pass = self.config.password.clone().unwrap_or_default();
        let db = self.config.default_database.clone();
        let opts = OptsBuilder::default()
            .ip_or_hostname(host)
            .tcp_port(port)
            .user(Some(user))
            .pass(Some(pass))
            .db_name(db);
        self.pool = Some(Pool::new(opts));
        let mut conn = self
            .pool()
            .unwrap()
            .get_conn()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        conn.query_drop("select 1")
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(pool) = self.pool.take() {
            pool.disconnect()
                .await
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
        }
        Ok(())
    }

    async fn version_string(&self) -> Result<String> {
        let mut conn = self
            .pool()?
            .get_conn()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let row: Option<(String,)> = conn
            .query_first("select version()")
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(row.map(|r| r.0).unwrap_or_else(|| "MySQL".into()))
    }

    async fn list_tables(&self) -> Result<Vec<TableOrView>> {
        self.list_rel("BASE TABLE", "table").await
    }

    async fn list_views(&self) -> Result<Vec<TableOrView>> {
        self.list_rel("VIEW", "view").await
    }

    async fn list_table_columns(&self, table: &str, _schema: Option<&str>) -> Result<Vec<TableColumn>> {
        let mut conn = self
            .pool()?
            .get_conn()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let rows: Vec<MysqlRow> = conn
            .exec(
                "select column_name, data_type, is_nullable, ordinal_position
                 from information_schema.columns
                 where table_name = ? and table_schema = database()
                 order by ordinal_position",
                (table,),
            )
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|row| TableColumn {
                column_name: row.get::<String, _>(0).unwrap_or_default(),
                data_type: row.get::<String, _>(1).unwrap_or_default(),
                nullable: row.get::<String, _>(2).unwrap_or_default() == "YES",
                ordinal_position: row.get::<u32, _>(3).unwrap_or(0) as i32,
            })
            .collect())
    }

    async fn execute_query(&self, sql: &str) -> Result<Vec<QueryResult>> {
        let mut conn = self
            .pool()?
            .get_conn()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        query_mysql(&mut conn, sql, Vec::new()).await
    }

    async fn select_top(&self, opts: SelectTop) -> Result<TableResult> {
        let dialect = Dialect::Mysql;
        let table = qualify(dialect, opts.schema.as_deref(), &opts.table)?;
        let where_clause = build_where(dialect, &opts.filters)?;
        let sql = format!(
            "SELECT {} FROM {table}{}{}{}",
            select_sql(dialect, &opts)?,
            where_clause.sql,
            order_sql(dialect, &opts)?,
            limit_sql(dialect, opts.limit.max(1), opts.offset.max(0))
        );
        let count_sql = format!("SELECT COUNT(*) FROM {table}{}", where_clause.sql);
        let params: Vec<mysql_async::Value> =
            where_clause.params.iter().map(json_to_mysql).collect();
        let mut conn = self
            .pool()?
            .get_conn()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let result = query_mysql(&mut conn, &sql, params.clone())
            .await?
            .into_iter()
            .next()
            .unwrap_or_default();
        let total: i64 = conn
            .exec_first(&count_sql, mysql_async::Params::Positional(params))
            .await
            .ok()
            .flatten()
            .unwrap_or(result.row_count as i64);
        Ok(TableResult {
            columns: result.columns,
            rows: result.rows,
            total,
        })
    }

    async fn apply_changes(&self, changes: TableChanges) -> Result<u64> {
        let dialect = Dialect::Mysql;
        let mut conn = self
            .pool()?
            .get_conn()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let mut affected = 0u64;
        for insert in &changes.inserts {
            let table = qualify(dialect, insert.schema.as_deref(), &insert.table)?;
            let cols: Result<Vec<String>> = insert
                .values
                .iter()
                .map(|(c, _)| dialect.quote_ident(c))
                .collect();
            let cols = cols?;
            let placeholders = vec!["?"; cols.len()].join(", ");
            let sql = format!("INSERT INTO {table} ({}) VALUES ({placeholders})", cols.join(", "));
            let params: Vec<mysql_async::Value> =
                insert.values.iter().map(|(_, v)| json_to_mysql(v)).collect();
            affected += conn
                .exec_drop(sql, mysql_async::Params::Positional(params))
                .await
                .map(|_| 1)
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
        }
        for update in &changes.updates {
            let table = qualify(dialect, update.schema.as_deref(), &update.table)?;
            let mut sets = Vec::new();
            let mut params = Vec::new();
            for (col, val) in &update.values {
                sets.push(format!("{} = ?", dialect.quote_ident(col)?));
                params.push(json_to_mysql(val));
            }
            let mut wheres = Vec::new();
            for (col, val) in &update.primary_keys {
                wheres.push(format!("{} = ?", dialect.quote_ident(col)?));
                params.push(json_to_mysql(val));
            }
            let sql = format!("UPDATE {table} SET {} WHERE {}", sets.join(", "), wheres.join(" AND "));
            conn.exec_drop(sql, mysql_async::Params::Positional(params))
                .await
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            affected += 1;
        }
        for delete in &changes.deletes {
            let table = qualify(dialect, delete.schema.as_deref(), &delete.table)?;
            let mut wheres = Vec::new();
            let mut params = Vec::new();
            for (col, val) in &delete.primary_keys {
                wheres.push(format!("{} = ?", dialect.quote_ident(col)?));
                params.push(json_to_mysql(val));
            }
            let sql = format!("DELETE FROM {table} WHERE {}", wheres.join(" AND "));
            conn.exec_drop(sql, mysql_async::Params::Positional(params))
                .await
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            affected += 1;
        }
        Ok(affected)
    }

    async fn get_primary_keys(&self, table: &str, _schema: Option<&str>) -> Result<Vec<String>> {
        let mut conn = self
            .pool()?
            .get_conn()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let rows: Vec<MysqlRow> = conn
            .exec(
                "select column_name from information_schema.key_column_usage
                 where table_name = ? and constraint_name = 'PRIMARY' and table_schema = database()",
                (table,),
            )
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|r| r.get::<String, _>(0))
            .collect())
    }

    async fn list_databases(&self) -> Result<Vec<String>> {
        let mut conn = self
            .pool()?
            .get_conn()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let rows: Vec<MysqlRow> = conn
            .query("show databases")
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|r| r.get::<String, _>(0))
            .collect())
    }
}

impl MysqlClient {
    async fn list_rel(&self, table_type: &str, entity: &str) -> Result<Vec<TableOrView>> {
        let mut conn = self
            .pool()?
            .get_conn()
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let rows: Vec<MysqlRow> = conn
            .exec(
                "select table_name from information_schema.tables
                 where table_schema = database() and table_type = ?
                 order by table_name",
                (table_type,),
            )
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(rows
            .into_iter()
            .filter_map(|row| row.get::<String, _>(0))
            .map(|name| TableOrView {
                name,
                schema: None,
                entity_type: entity.into(),
            })
            .collect())
    }
}

async fn query_mysql(
    conn: &mut mysql_async::Conn,
    sql: &str,
    params: Vec<mysql_async::Value>,
) -> Result<Vec<QueryResult>> {
    match conn
        .exec::<MysqlRow, _, _>(sql, mysql_async::Params::Positional(params.clone()))
        .await
    {
        Ok(rows) => Ok(vec![mysql_rows_to_result(rows)]),
        Err(_) => {
            conn.exec_drop(sql, mysql_async::Params::Positional(params))
                .await
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            Ok(vec![QueryResult {
                columns: vec!["ok".into()],
                rows: vec![vec![json!(true)]],
                row_count: 1,
                truncated: false,
            }])
        }
    }
}

fn mysql_rows_to_result(rows: Vec<MysqlRow>) -> QueryResult {
    let columns: Vec<String> = rows
        .first()
        .map(|row| {
            row.columns_ref()
                .iter()
                .map(|c| c.name_str().to_string())
                .collect()
        })
        .unwrap_or_default();
    let mut out = Vec::new();
    for row in rows.into_iter().take(5000) {
        let mut values = Vec::new();
        for i in 0..columns.len() {
            let raw = row.as_ref(i).cloned().unwrap_or(mysql_async::Value::NULL);
            values.push(mysql_to_json(raw));
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
