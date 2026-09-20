use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use graphite_core::Dialect;
use graphite_core::{GraphiteError, Result};
use graphite_core::build_where;
use graphite_core::{
    ConnectionConfig, DatabaseClient, QueryResult, SelectTop, TableChanges, TableColumn,
    TableOrView, TableResult,
};
use rusqlite::{params_from_iter, Connection};

use crate::sqlutil::{limit_sql, qualify};
use crate::value::{json_to_sqlite, sqlite_to_json};

pub struct SqliteClient {
    filename: String,
    conn: Option<Arc<Mutex<Connection>>>,
}

impl SqliteClient {
    pub fn new(config: ConnectionConfig) -> Result<Self> {
        let filename = config
            .filename
            .filter(|f| !f.is_empty())
            .unwrap_or_else(|| ":memory:".into());
        Ok(Self {
            filename,
            conn: None,
        })
    }

    fn conn(&self) -> Result<Arc<Mutex<Connection>>> {
        self.conn
            .clone()
            .ok_or(GraphiteError::NotConnected)
    }
}

#[async_trait]
impl DatabaseClient for SqliteClient {
    async fn connect(&mut self) -> Result<()> {
        let filename = self.filename.clone();
        let connection = tokio::task::spawn_blocking(move || Connection::open(filename))
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        self.conn = Some(Arc::new(Mutex::new(connection)));
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.conn = None;
        Ok(())
    }

    async fn version_string(&self) -> Result<String> {
        let conn = self.conn()?;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| GraphiteError::msg(e.to_string()))?;
            let v: String = conn
                .query_row("select sqlite_version()", [], |r| r.get(0))
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            Ok(format!("SQLite {v}"))
        })
        .await
        .map_err(|e| GraphiteError::msg(e.to_string()))?
    }

    async fn list_tables(&self) -> Result<Vec<TableOrView>> {
        self.list_kind("table").await
    }

    async fn list_views(&self) -> Result<Vec<TableOrView>> {
        self.list_kind("view").await
    }

    async fn list_table_columns(&self, table: &str, _schema: Option<&str>) -> Result<Vec<TableColumn>> {
        let conn = self.conn()?;
        let table = table.to_string();
        graphite_core::validate_ident(&table)?;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| GraphiteError::msg(e.to_string()))?;
            let mut stmt = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(TableColumn {
                        column_name: row.get::<_, String>(1)?,
                        data_type: row.get::<_, String>(2)?,
                        nullable: row.get::<_, i64>(3)? == 0,
                        ordinal_position: row.get::<_, i64>(0)? as i32,
                    })
                })
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| GraphiteError::msg(e.to_string()))
        })
        .await
        .map_err(|e| GraphiteError::msg(e.to_string()))?
    }

    async fn execute_query(&self, sql: &str) -> Result<Vec<QueryResult>> {
        let conn = self.conn()?;
        let sql = sql.to_string();
        tokio::task::spawn_blocking(move || run_sql(&conn, &sql))
            .await
            .map_err(|e| GraphiteError::msg(e.to_string()))?
    }

    async fn select_top(&self, opts: SelectTop) -> Result<TableResult> {
        let dialect = Dialect::Sqlite;
        let table = qualify(dialect, opts.schema.as_deref(), &opts.table)?;
        let where_clause = build_where(dialect, &opts.filters)?;
        let order = if opts.order_by.is_empty() {
            String::new()
        } else {
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
            format!(" ORDER BY {}", bits?.join(", "))
        };
        let cols = if let Some(selects) = &opts.selects {
            let quoted: Result<Vec<String>> = selects.iter().map(|c| dialect.quote_ident(c)).collect();
            quoted?.join(", ")
        } else {
            "*".into()
        };
        let limit = limit_sql(dialect, opts.limit.max(1), opts.offset.max(0));
        let sql = format!("SELECT {cols} FROM {table}{}{order}{limit}", where_clause.sql);
        let count_sql = format!("SELECT COUNT(*) FROM {table}{}", where_clause.sql);
        let conn = self.conn()?;
        let params = where_clause.params.clone();
        tokio::task::spawn_blocking(move || {
            let result = run_sql_with_params(&conn, &sql, &params)?;
            let first = result.into_iter().next().unwrap_or_default();
            let total = {
                let conn = conn.lock().map_err(|e| GraphiteError::msg(e.to_string()))?;
                let mut stmt = conn
                    .prepare(&count_sql)
                    .map_err(|e| GraphiteError::msg(e.to_string()))?;
                let bind = params.iter().map(json_to_sqlite).collect::<Vec<_>>();
                stmt.query_row(params_from_iter(bind.iter()), |row| row.get::<_, i64>(0))
                    .unwrap_or(first.row_count as i64)
            };
            Ok(TableResult {
                columns: first.columns,
                rows: first.rows,
                total,
            })
        })
        .await
        .map_err(|e| GraphiteError::msg(e.to_string()))?
    }

    async fn apply_changes(&self, changes: TableChanges) -> Result<u64> {
        let conn = self.conn()?;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| GraphiteError::msg(e.to_string()))?;
            let tx = conn
                .unchecked_transaction()
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            let mut affected = 0u64;
            for insert in &changes.inserts {
                let table = Dialect::Sqlite.quote_ident(&insert.table)?;
                let cols: Result<Vec<String>> = insert
                    .values
                    .iter()
                    .map(|(c, _)| Dialect::Sqlite.quote_ident(c))
                    .collect();
                let cols = cols?;
                let placeholders = vec!["?"; cols.len()].join(", ");
                let sql = format!(
                    "INSERT INTO {table} ({}) VALUES ({placeholders})",
                    cols.join(", ")
                );
                let bind = insert.values.iter().map(|(_, v)| json_to_sqlite(v)).collect::<Vec<_>>();
                affected += tx
                    .execute(&sql, params_from_iter(bind.iter()))
                    .map_err(|e| GraphiteError::msg(e.to_string()))? as u64;
            }
            for update in &changes.updates {
                let table = Dialect::Sqlite.quote_ident(&update.table)?;
                let set_cols: Result<Vec<String>> = update
                    .values
                    .iter()
                    .map(|(c, _)| Ok(format!("{} = ?", Dialect::Sqlite.quote_ident(c)?)))
                    .collect();
                let where_cols: Result<Vec<String>> = update
                    .primary_keys
                    .iter()
                    .map(|(c, _)| Ok(format!("{} = ?", Dialect::Sqlite.quote_ident(c)?)))
                    .collect();
                let sql = format!(
                    "UPDATE {table} SET {} WHERE {}",
                    set_cols?.join(", "),
                    where_cols?.join(" AND ")
                );
                let mut bind = Vec::new();
                bind.extend(update.values.iter().map(|(_, v)| json_to_sqlite(v)));
                bind.extend(update.primary_keys.iter().map(|(_, v)| json_to_sqlite(v)));
                affected += tx
                    .execute(&sql, params_from_iter(bind.iter()))
                    .map_err(|e| GraphiteError::msg(e.to_string()))? as u64;
            }
            for delete in &changes.deletes {
                let table = Dialect::Sqlite.quote_ident(&delete.table)?;
                let where_cols: Result<Vec<String>> = delete
                    .primary_keys
                    .iter()
                    .map(|(c, _)| Ok(format!("{} = ?", Dialect::Sqlite.quote_ident(c)?)))
                    .collect();
                let sql = format!("DELETE FROM {table} WHERE {}", where_cols?.join(" AND "));
                let bind = delete
                    .primary_keys
                    .iter()
                    .map(|(_, v)| json_to_sqlite(v))
                    .collect::<Vec<_>>();
                affected += tx
                    .execute(&sql, params_from_iter(bind.iter()))
                    .map_err(|e| GraphiteError::msg(e.to_string()))? as u64;
            }
            tx.commit().map_err(|e| GraphiteError::msg(e.to_string()))?;
            Ok(affected)
        })
        .await
        .map_err(|e| GraphiteError::msg(e.to_string()))?
    }

    async fn get_primary_keys(&self, table: &str, _schema: Option<&str>) -> Result<Vec<String>> {
        let cols = self.list_table_columns(table, None).await?;
        Ok(cols
            .into_iter()
            .filter(|c| c.column_name.eq_ignore_ascii_case("id") || c.ordinal_position == 0)
            .map(|c| c.column_name)
            .collect())
    }

    async fn list_databases(&self) -> Result<Vec<String>> {
        Ok(vec!["main".into()])
    }
}

impl SqliteClient {
    async fn list_kind(&self, kind: &str) -> Result<Vec<TableOrView>> {
        let conn = self.conn()?;
        let kind = kind.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().map_err(|e| GraphiteError::msg(e.to_string()))?;
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type = ?1 AND name NOT LIKE 'sqlite_%' ORDER BY name")
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            let rows = stmt
                .query_map([&kind], |row| {
                    Ok(TableOrView {
                        name: row.get(0)?,
                        schema: Some("main".into()),
                        entity_type: kind.clone(),
                    })
                })
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| GraphiteError::msg(e.to_string()))
        })
        .await
        .map_err(|e| GraphiteError::msg(e.to_string()))?
    }
}

fn run_sql(conn: &Arc<Mutex<Connection>>, sql: &str) -> Result<Vec<QueryResult>> {
    run_sql_with_params(conn, sql, &[])
}

fn run_sql_with_params(
    conn: &Arc<Mutex<Connection>>,
    sql: &str,
    params: &[serde_json::Value],
) -> Result<Vec<QueryResult>> {
    let conn = conn.lock().map_err(|e| GraphiteError::msg(e.to_string()))?;
    let bind = params.iter().map(json_to_sqlite).collect::<Vec<_>>();
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| GraphiteError::msg(e.to_string()))?;
    if stmt.column_count() == 0 {
        conn.execute(sql, params_from_iter(bind.iter()))
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        return Ok(vec![QueryResult {
            columns: vec!["rows".into()],
            rows: vec![vec![serde_json::json!(conn.changes())]],
            row_count: 1,
            truncated: false,
        }]);
    }
    let columns: Vec<String> = stmt.column_names().into_iter().map(|s| s.to_string()).collect();
    let mut rows = Vec::new();
    let mut query = stmt
        .query(params_from_iter(bind.iter()))
        .map_err(|e| GraphiteError::msg(e.to_string()))?;
    while let Some(row) = query.next().map_err(|e| GraphiteError::msg(e.to_string()))? {
        let mut values = Vec::new();
        for i in 0..columns.len() {
            values.push(sqlite_to_json(row.get_ref(i).map_err(|e| GraphiteError::msg(e.to_string()))?));
        }
        rows.push(values);
        if rows.len() >= 5000 {
            break;
        }
    }
    let truncated = rows.len() >= 5000;
    let row_count = rows.len();
    Ok(vec![QueryResult {
        columns,
        rows,
        row_count,
        truncated,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphite_core::{OrderBy, RowChange, TableFilter};
    use serde_json::json;

    #[tokio::test]
    async fn sqlite_query_filter_and_edit() {
        let mut client = SqliteClient::new(ConnectionConfig {
            connection_type: "sqlite".into(),
            filename: Some(":memory:".into()),
            ..Default::default()
        })
        .unwrap();
        client.connect().await.unwrap();
        client
            .execute_query("create table items (id integer primary key, name text, amount integer)")
            .await
            .unwrap();
        client
            .apply_changes(TableChanges {
                inserts: vec![
                    RowChange {
                        table: "items".into(),
                        values: vec![
                            ("name".into(), json!("a")),
                            ("amount".into(), json!(1)),
                        ],
                        ..Default::default()
                    },
                    RowChange {
                        table: "items".into(),
                        values: vec![
                            ("name".into(), json!("b")),
                            ("amount".into(), json!(5)),
                        ],
                        ..Default::default()
                    },
                    RowChange {
                        table: "items".into(),
                        values: vec![
                            ("name".into(), json!("c")),
                            ("amount".into(), json!(9)),
                        ],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            })
            .await
            .unwrap();
        let top = client
            .select_top(SelectTop {
                table: "items".into(),
                limit: 50,
                filters: vec![
                    TableFilter {
                        field: "amount".into(),
                        op: ">".into(),
                        value: json!(1),
                    },
                    TableFilter {
                        field: "amount".into(),
                        op: "<".into(),
                        value: json!(9),
                    },
                    TableFilter {
                        field: "name".into(),
                        op: "like".into(),
                        value: json!("%b%"),
                    },
                ],
                order_by: vec![OrderBy {
                    field: "id".into(),
                    dir: "asc".into(),
                }],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(top.total, 1);
        assert_eq!(top.rows[0][1], json!("b"));
    }
}
