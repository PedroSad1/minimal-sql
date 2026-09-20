use graphite_core::Dialect;
use graphite_core::Result;

pub fn qualify(dialect: Dialect, schema: Option<&str>, table: &str) -> Result<String> {
    let table_sql = dialect.quote_ident(table)?;
    if let Some(schema) = schema.filter(|s| !s.is_empty()) {
        let schema_sql = dialect.quote_ident(schema)?;
        Ok(format!("{schema_sql}.{table_sql}"))
    } else {
        Ok(table_sql)
    }
}

pub fn limit_sql(dialect: Dialect, limit: i64, offset: i64) -> String {
    match dialect {
        Dialect::Sqlserver => format!(" OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY"),
        _ => format!(" LIMIT {limit} OFFSET {offset}"),
    }
}
