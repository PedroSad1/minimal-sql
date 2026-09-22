    use std::sync::Arc;

    use async_trait::async_trait;
    use graphite_core::Dialect;
    use graphite_core::{GraphiteError, Result};
    use graphite_core::build_where;
    use graphite_core::{
        ConnectionConfig, DatabaseClient, QueryResult, Routine, SelectTop, TableChanges,
        TableColumn, TableIndex, TableOrView, TableResult, TableTrigger,
    };
    use serde_json::{json, Value};
    use tokio_postgres::types::{FromSql, Kind, Type};
    use tokio_postgres::{NoTls, Row, SimpleQueryMessage};

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
            let sslmode = if self.config.ssl { "require" } else { "disable" };
            format!(
                "host={host} port={port} user={user} password={pass} dbname={db} sslmode={sslmode}"
            )
        }

        fn client(&self) -> Result<Arc<tokio_postgres::Client>> {
            self.client.clone().ok_or(GraphiteError::NotConnected)
        }
    }

    #[async_trait]
    impl DatabaseClient for PostgresClient {
        async fn connect(&mut self) -> Result<()> {
            if self.config.ssl {
                // Match DBeaver/libpq `sslmode=require`: encrypt, do not require a public CA.
                let connector = native_tls::TlsConnector::builder()
                    .danger_accept_invalid_certs(true)
                    .danger_accept_invalid_hostnames(true)
                    .build()
                    .map_err(|e| GraphiteError::msg(e.to_string()))?;
                let connector = postgres_native_tls::MakeTlsConnector::new(connector);
                let (client, connection) = tokio_postgres::connect(&self.url(), connector)
                    .await
                    .map_err(|e| GraphiteError::msg(e.to_string()))?;
                tokio::spawn(async move {
                    let _ = connection.await;
                });
                self.client = Some(Arc::new(client));
            } else {
                let (client, connection) = tokio_postgres::connect(&self.url(), NoTls)
                    .await
                    .map_err(|e| GraphiteError::msg(e.to_string()))?;
                tokio::spawn(async move {
                    let _ = connection.await;
                });
                self.client = Some(Arc::new(client));
            }
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
            let client = self.client()?;
            let rows = client
                .query(
                    "select t.table_schema, t.table_name,
                            max(parent_pc.relname) filter (where parent_pc.relkind = 'p') as parent_name
                     from information_schema.tables t
                     join pg_namespace ns on ns.nspname = t.table_schema
                     join pg_class pc on pc.relname = t.table_name and pc.relnamespace = ns.oid
                     left join pg_inherits i on i.inhrelid = pc.oid
                     left join pg_class parent_pc on parent_pc.oid = i.inhparent
                     where t.table_type = 'BASE TABLE'
                       and t.table_schema not in ('pg_catalog', 'information_schema')
                     group by t.table_schema, t.table_name
                     order by t.table_schema, t.table_name",
                    &[],
                )
                .await
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            Ok(rows
                .into_iter()
                .map(|row| TableOrView {
                    schema: Some(row.get(0)),
                    name: row.get(1),
                    entity_type: "table".into(),
                    parent: row.get(2),
                })
                .collect())
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
            let messages = client
                .simple_query(sql)
                .await
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            Ok(simple_messages_to_results(&messages))
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
            let client = self.client()?;
            let params: Vec<SqlVal> = where_clause.params.iter().map(SqlVal::from).collect();
            let result = query_pg(&client, &sql, &params).await?.into_iter().next().unwrap_or_default();
            let total = if opts.skip_count {
                result.row_count as i64
            } else {
                let count_sql = format!("SELECT COUNT(*) FROM {table}{}", where_clause.sql);
                let count_params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                    params.iter().map(|p| p as _).collect();
                client
                    .query_one(&count_sql, &count_params)
                    .await
                    .ok()
                    .and_then(|row| row.try_get::<_, i64>(0).ok())
                    .unwrap_or(result.row_count as i64)
            };
            Ok(TableResult {
                columns: result.columns,
                rows: result.rows,
                enum_values: result.enum_values,
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

        async fn list_schemas(&self) -> Result<Vec<String>> {
            let client = self.client()?;
            let rows = client
                .query(
                    "select schema_name from information_schema.schemata
                    where schema_name not like 'pg_%' and schema_name <> 'information_schema'
                    order by schema_name",
                    &[],
                )
                .await
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            Ok(rows.into_iter().map(|row| row.get(0)).collect())
        }

        async fn list_routines(&self) -> Result<Vec<Routine>> {
            let client = self.client()?;
            let rows = client
                .query(
                    "select routine_name, routine_schema, routine_type
                    from information_schema.routines
                    where routine_schema not in ('pg_catalog', 'information_schema')
                    order by routine_schema, routine_name",
                    &[],
                )
                .await
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            Ok(rows
                .into_iter()
                .map(|row| Routine {
                    name: row.get(0),
                    schema: row.get(1),
                    routine_type: row.get(2),
                })
                .collect())
        }

        async fn list_table_indexes(&self, table: &str, schema: Option<&str>) -> Result<Vec<TableIndex>> {
            let client = self.client()?;
            let schema = schema.unwrap_or("public");
            let rows = client
                .query(
                    "select indexname, indexdef
                    from pg_indexes
                    where tablename = $1 and schemaname = $2
                    order by indexname",
                    &[&table, &schema],
                )
                .await
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            Ok(rows
                .into_iter()
                .map(|row| {
                    let name: String = row.get(0);
                    let def: String = row.get(1);
                    TableIndex {
                        unique: def.to_uppercase().contains("UNIQUE"),
                        primary: def.to_uppercase().contains("PRIMARY"),
                        columns: Vec::new(),
                        name,
                    }
                })
                .collect())
        }

        async fn list_table_triggers(&self, table: &str, schema: Option<&str>) -> Result<Vec<TableTrigger>> {
            let client = self.client()?;
            let schema = schema.unwrap_or("public");
            let rows = client
                .query(
                    "select trigger_name, action_timing, event_manipulation
                    from information_schema.triggers
                    where event_object_table = $1 and event_object_schema = $2
                    order by trigger_name",
                    &[&table, &schema],
                )
                .await
                .map_err(|e| GraphiteError::msg(e.to_string()))?;
            Ok(rows
                .into_iter()
                .map(|row| TableTrigger {
                    name: row.get(0),
                    timing: row.get(1),
                    manipulation: row.get(2),
                })
                .collect())
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
                    parent: None,
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
        match client.prepare(sql).await {
            Ok(stmt) => {
                let rows = client
                    .query(&stmt, &refs)
                    .await
                    .map_err(|e| GraphiteError::msg(e.to_string()))?;
                let mut result = rows_to_result(stmt.columns(), &rows);
                result.enum_values = load_enum_labels(client, stmt.columns(), &result.enum_values).await;
                Ok(vec![result])
            }
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
                    enum_values: Vec::new(),
                }])
            }
        }
    }

    fn simple_messages_to_results(messages: &[SimpleQueryMessage]) -> Vec<QueryResult> {
        let mut results = Vec::new();
        let mut columns: Option<Vec<String>> = None;
        let mut rows: Vec<Vec<Value>> = Vec::new();

        let flush = |results: &mut Vec<QueryResult>,
                     columns: &mut Option<Vec<String>>,
                     rows: &mut Vec<Vec<Value>>| {
            if let Some(cols) = columns.take() {
                let truncated = rows.len() > 5000;
                rows.truncate(5000);
                results.push(QueryResult {
                    row_count: rows.len(),
                    truncated,
                    columns: cols,
                    rows: std::mem::take(rows),
                    enum_values: Vec::new(),
                });
            }
        };

        for message in messages {
            match message {
                SimpleQueryMessage::RowDescription(cols) => {
                    flush(&mut results, &mut columns, &mut rows);
                    columns = Some(cols.iter().map(|col| col.name().to_string()).collect());
                }
                SimpleQueryMessage::Row(row) => {
                    let width = columns.as_ref().map(|cols| cols.len()).unwrap_or(row.len());
                    let values = (0..width).map(|i| json!(row.get(i))).collect();
                    rows.push(values);
                }
                SimpleQueryMessage::CommandComplete(count) => {
                    if columns.is_some() {
                        flush(&mut results, &mut columns, &mut rows);
                    } else {
                        results.push(QueryResult {
                            columns: vec!["rows".into()],
                            rows: vec![vec![json!(count)]],
                            row_count: 1,
                            truncated: false,
                            enum_values: Vec::new(),
                        });
                    }
                }
                _ => {}
            }
        }
        flush(&mut results, &mut columns, &mut rows);
        if results.is_empty() {
            results.push(QueryResult {
                columns: vec!["status".into()],
                rows: vec![vec![json!("ok")]],
                row_count: 1,
                truncated: false,
                enum_values: Vec::new(),
            });
        }
        results
    }

    fn rows_to_result(columns: &[tokio_postgres::Column], rows: &[Row]) -> QueryResult {
        let names: Vec<String> = columns.iter().map(|c| c.name().to_string()).collect();
        let width = names.len();
        let mut out = Vec::new();
        for row in rows.iter().take(5000) {
            let mut values = Vec::new();
            for i in 0..width {
                values.push(pg_cell(row, i));
            }
            out.push(values);
        }
        QueryResult {
            row_count: out.len(),
            truncated: rows.len() > 5000,
            columns: names,
            rows: out,
            enum_values: columns.iter().map(|column| labels_for(column.type_())).collect(),
        }
    }

    fn pg_cell(row: &Row, i: usize) -> Value {
        if let Ok(v) = row.try_get::<_, Option<bool>>(i) {
            return json!(v);
        }
        if let Ok(v) = row.try_get::<_, Option<i16>>(i) {
            return json!(v);
        }
        if let Ok(v) = row.try_get::<_, Option<i32>>(i) {
            return json!(v);
        }
        if let Ok(v) = row.try_get::<_, Option<i64>>(i) {
            return json!(v);
        }
        if let Ok(v) = row.try_get::<_, Option<f32>>(i) {
            return json!(v);
        }
        if let Ok(v) = row.try_get::<_, Option<f64>>(i) {
            return json!(v);
        }
        if let Ok(v) = row.try_get::<_, Option<String>>(i) {
            return json!(v);
        }
        if let Ok(v) = row.try_get::<_, Option<chrono::NaiveDateTime>>(i) {
            return json!(v.map(|d| d.to_string()));
        }
        if let Ok(v) = row.try_get::<_, Option<chrono::NaiveDate>>(i) {
            return json!(v.map(|d| d.to_string()));
        }
        if let Ok(v) = row.try_get::<_, Option<chrono::NaiveTime>>(i) {
            return json!(v.map(|d| d.to_string()));
        }
        if let Ok(v) = row.try_get::<_, Option<chrono::DateTime<chrono::Utc>>>(i) {
            return json!(v.map(|d| d.to_rfc3339()));
        }
        if let Ok(v) = row.try_get::<_, Option<uuid::Uuid>>(i) {
            return json!(v.map(|u| u.to_string()));
        }
        if let Ok(v) = row.try_get::<_, Option<serde_json::Value>>(i) {
            return v.unwrap_or(Value::Null);
        }
        if let Ok(tokio_postgres::types::Json(v)) =
            row.try_get::<_, tokio_postgres::types::Json<Value>>(i)
        {
            return v;
        }
        if let Ok(v) = row.try_get::<_, Option<Vec<u8>>>(i) {
            return json!(v.map(|bytes| format!(
                "\\x{}",
                bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
            )));
        }
        if let Ok(v) = row.try_get::<_, Option<PgLabel>>(i) {
            return json!(v.map(|label| label.0));
        }
        Value::Null
    }

    struct PgLabel(String);

    impl<'a> FromSql<'a> for PgLabel {
        fn from_sql(
            _ty: &Type,
            raw: &'a [u8],
        ) -> std::result::Result<Self, Box<dyn std::error::Error + Sync + Send>> {
            let text = std::str::from_utf8(raw)?;
            Ok(PgLabel(text.to_string()))
        }

        fn accepts(ty: &Type) -> bool {
            enum_label(ty)
        }
    }

    fn enum_label(ty: &Type) -> bool {
        labels_for(ty).is_some()
    }

    fn labels_for(ty: &Type) -> Option<Vec<String>> {
        match ty.kind() {
            Kind::Enum(values) if !values.is_empty() => Some(values.clone()),
            Kind::Domain(inner) => labels_for(inner),
            _ => None,
        }
    }

    async fn load_enum_labels(
        client: &tokio_postgres::Client,
        columns: &[tokio_postgres::Column],
        current: &[Option<Vec<String>>],
    ) -> Vec<Option<Vec<String>>> {
        let mut out = current.to_vec();
        out.resize(columns.len(), None);
        let mut missing = Vec::new();
        for (index, column) in columns.iter().enumerate() {
            if out[index].as_ref().is_some_and(|labels| !labels.is_empty()) {
                continue;
            }
            let ty = column.type_();
            if let Some(labels) = labels_for(ty) {
                out[index] = Some(labels);
                continue;
            }
            if ty.schema() != "pg_catalog" {
                missing.push((index, ty.oid()));
            }
        }
        if missing.is_empty() {
            return out;
        }
        let ids = missing
            .iter()
            .map(|(_, oid)| oid.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT enumtypid, enumlabel FROM pg_catalog.pg_enum WHERE enumtypid IN ({ids}) ORDER BY enumtypid, enumsortorder"
        );
        let Ok(rows) = client.query(&sql, &[]).await else {
            return out;
        };
        let mut grouped: std::collections::HashMap<u32, Vec<String>> = std::collections::HashMap::new();
        for row in rows {
            let Ok(oid) = row.try_get::<_, u32>(0) else {
                continue;
            };
            let Ok(label) = row.try_get::<_, String>(1) else {
                continue;
            };
            grouped.entry(oid).or_default().push(label);
        }
        for (index, oid) in missing {
            if let Some(labels) = grouped.get(&oid) {
                if !labels.is_empty() {
                    out[index] = Some(labels.clone());
                }
            }
        }
        out
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn enum_label_decodes_as_text() {
            let ty = Type::new(
                "status".into(),
                16_384,
                Kind::Enum(vec!["open".into(), "closed".into()]),
                "public".into(),
            );
            assert!(<PgLabel as FromSql>::accepts(&ty));
            assert!(!<&str as FromSql>::accepts(&ty));
            assert_eq!(
                labels_for(&ty).as_deref(),
                Some(["open".to_string(), "closed".to_string()].as_slice())
            );
            let label = PgLabel::from_sql(&ty, b"open").unwrap();
            assert_eq!(label.0, "open");

            let domain = Type::new("status_d".into(), 16_385, Kind::Domain(ty), "public".into());
            assert!(<PgLabel as FromSql>::accepts(&domain));
            let through = PgLabel::from_sql(&domain, b"closed").unwrap();
            assert_eq!(through.0, "closed");
        }
    }
