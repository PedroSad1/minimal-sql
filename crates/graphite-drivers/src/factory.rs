use graphite_core::{GraphiteError, Result};
use graphite_core::{ConnectionConfig, DatabaseClient};

use crate::bigquery::BigQueryClient;
use crate::mssql::MssqlClient;
use crate::mysql::MysqlClient;
use crate::postgres::PostgresClient;
use crate::redis_client::RedisClient;
use crate::sqlite::SqliteClient;

pub async fn open_client(config: ConnectionConfig) -> Result<Box<dyn DatabaseClient>> {
    let kind = config.connection_type.as_str();
    let client: Box<dyn DatabaseClient> = match kind {
        "sqlite" => Box::new(SqliteClient::new(config)?),
        "postgresql" | "cockroachdb" | "redshift" | "greengage" => {
            Box::new(PostgresClient::new(config))
        }
        "mysql" | "mariadb" | "tidb" | "starrocks" | "bedrock" => {
            Box::new(MysqlClient::new(config))
        }
        "sqlserver" => Box::new(MssqlClient::new(config)),
        "redis" => Box::new(RedisClient::new(config)),
        "bigquery" => Box::new(BigQueryClient::new(config)),
        other => return Err(GraphiteError::UnsupportedType(other.to_string())),
    };
    Ok(client)
}
