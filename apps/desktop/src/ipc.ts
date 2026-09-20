import { invoke } from "@tauri-apps/api/core";

/** Keep Beekeeper Community channel names (`conn/listTables`) mapped to Tauri commands. */
export function send<T>(channel: string, payload: Record<string, unknown> = {}): Promise<T> {
  const command = channel.replace(/\//g, "_").replace(/[A-Z]/g, (ch) => `_${ch.toLowerCase()}`);
  return invoke<T>(command, payload);
}

export const ipc = {
  create: (config: unknown) => invoke<string>("conn_create", { config }),
  disconnect: (sessionId: string) => invoke("conn_disconnect", { sessionId }),
  version: (sessionId: string) => invoke<string>("conn_version_string", { sessionId }),
  tables: (sessionId: string) => invoke<TableOrView[]>("conn_list_tables", { sessionId }),
  views: (sessionId: string) => invoke<TableOrView[]>("conn_list_views", { sessionId }),
  columns: (sessionId: string, table: string, schema?: string) =>
    invoke<TableColumn[]>("conn_list_table_columns", { sessionId, table, schema }),
  query: (sessionId: string, sql: string) =>
    invoke<QueryResult[]>("query_execute", { sessionId, sql }),
  selectTop: (sessionId: string, opts: unknown) =>
    invoke<TableResult>("conn_select_top", { sessionId, opts }),
  applyChanges: (sessionId: string, changes: unknown) =>
    invoke<number>("conn_apply_changes", { sessionId, changes }),
  exportResult: (result: QueryResult, path: string, format: string) =>
    invoke("export_result", { result, path, format }),
  queryToFile: (sessionId: string, sql: string, path: string, format: string) =>
    invoke("query_execute_to_file", { sessionId, sql, path, format }),
  importFile: (path: string, format: string) => invoke<ImportedTable>("import_file", { path, format }),
  backup: (sessionId: string, table: string, path: string) =>
    invoke("backup_table", { sessionId, table, path }),
  savedFind: () => invoke<SavedConnection[]>("appdb_saved_find"),
  savedSave: (obj: SavedConnection) => invoke("appdb_saved_save", { obj }),
  history: () => invoke<string[]>("appdb_history_find"),
  getSetting: (key: string) => invoke<string | null>("appdb_setting_get", { key }),
  setSetting: (key: string, value: string) => invoke("appdb_setting_set", { key, value }),
};

export interface TableOrView {
  name: string;
  schema?: string | null;
  entityType: string;
}

export interface TableColumn {
  columnName: string;
  dataType: string;
  nullable: boolean;
  ordinalPosition: number;
}

export interface QueryResult {
  columns: string[];
  rows: unknown[][];
  rowCount: number;
  truncated: boolean;
}

export interface TableResult {
  columns: string[];
  rows: unknown[][];
  total: number;
}

export interface ImportedTable {
  columns: string[];
  rows: unknown[][];
}

export interface SavedConnection {
  id: string;
  name: string;
  payload: unknown;
}

export const COMMUNITY_TYPES = [
  { value: "sqlite", label: "SQLite" },
  { value: "postgresql", label: "PostgreSQL" },
  { value: "cockroachdb", label: "CockroachDB" },
  { value: "redshift", label: "Amazon Redshift" },
  { value: "greengage", label: "GreengageDB" },
  { value: "mysql", label: "MySQL" },
  { value: "mariadb", label: "MariaDB" },
  { value: "tidb", label: "TiDB" },
  { value: "starrocks", label: "StarRocks" },
  { value: "bedrock", label: "Bedrock" },
  { value: "sqlserver", label: "SQL Server" },
  { value: "redis", label: "Redis" },
  { value: "bigquery", label: "Google BigQuery" },
];
