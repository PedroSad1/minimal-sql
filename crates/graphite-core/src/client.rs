use crate::error::Result;
use crate::types::*;
use async_trait::async_trait;

#[async_trait]
pub trait DatabaseClient: Send + Sync {
    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self) -> Result<()>;
    async fn version_string(&self) -> Result<String>;
    async fn list_tables(&self) -> Result<Vec<TableOrView>>;
    async fn list_views(&self) -> Result<Vec<TableOrView>>;
    async fn list_table_columns(&self, table: &str, schema: Option<&str>) -> Result<Vec<TableColumn>>;
    async fn execute_query(&self, sql: &str) -> Result<Vec<QueryResult>>;
    async fn select_top(&self, opts: SelectTop) -> Result<TableResult>;
    async fn apply_changes(&self, changes: TableChanges) -> Result<u64>;
    async fn get_primary_keys(&self, table: &str, schema: Option<&str>) -> Result<Vec<String>>;
    async fn list_databases(&self) -> Result<Vec<String>>;
    async fn supported_features(&self) -> Result<SupportedFeatures> {
        Ok(SupportedFeatures::all())
    }
    async fn default_schema(&self) -> Result<Option<String>> {
        Ok(None)
    }
    async fn list_schemas(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
    async fn list_routines(&self) -> Result<Vec<Routine>> {
        Ok(Vec::new())
    }
    async fn list_table_indexes(&self, _table: &str, _schema: Option<&str>) -> Result<Vec<TableIndex>> {
        Ok(Vec::new())
    }
    async fn list_table_triggers(&self, _table: &str, _schema: Option<&str>) -> Result<Vec<TableTrigger>> {
        Ok(Vec::new())
    }
}
