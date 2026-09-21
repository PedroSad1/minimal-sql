import { send, setSessionId, getSessionId } from "./tauri-util";

export { send, setSessionId, getSessionId };

export const ipc = {
  create: (config: unknown) => send<string>("conn/create", { config }),
  connect: () => send("conn/connect"),
  disconnect: () => send("conn/disconnect"),
  version: () => send<string>("conn/versionString"),
  tables: () => send<TableOrView[]>("conn/listTables"),
  views: () => send<TableOrView[]>("conn/listViews"),
  columns: (table: string, schema?: string) =>
    send<TableColumn[]>("conn/listTableColumns", { table, schema }),
  indexes: (table: string, schema?: string) =>
    send<TableIndex[]>("conn/listTableIndexes", { table, schema }),
  triggers: (table: string, schema?: string) =>
    send<TableTrigger[]>("conn/listTableTriggers", { table, schema }),
  query: (sql: string) => send<NgQueryResult[]>("conn/executeQuery", { queryText: sql }),
  exportQuery: (sql: string, path: string, format: string) =>
    send<{ path: string }>("conn/exportQuery", { sql, path, format }),
  importFile: (path: string, format: string) =>
    send<ImportedTable>("conn/importFile", { path, format }),
  selectTop: (opts: unknown) => send<NgQueryResult>("conn/selectTop", { opts }),
  applyChanges: (changes: TableChanges) => send<number>("conn/applyChanges", { changes }),
  primaryKeys: (table: string, schema?: string | null) =>
    send<string[]>("conn/getPrimaryKeys", { table, schema: schema ?? undefined }),
  savedFind: () => send<SavedConnection[]>("appdb/saved/find"),
  savedSave: (obj: SavedConnection) => send<SavedConnection>("appdb/saved/save", { obj }),
  savedRemove: (id: string) => send<void>("appdb/saved/remove", { id }),
  history: () => send<string[]>("appdb/history/find"),
  getSetting: (key: string) => send<string | null>("appdb/setting/get", { key }),
  setSetting: (key: string, value: string) => send("appdb/setting/set", { key, value }),
};

export interface FieldDescriptor {
  name: string;
  id: string;
  dataType?: string | null;
}

export interface NgQueryResult {
  fields?: FieldDescriptor[];
  rows?: Record<string, unknown>[];
  rowCount?: number;
  totalRowCount?: number;
  truncated?: boolean;
  command?: string;
  affectedRows?: number;
  text?: string;
}

export interface TableOrView {
  name: string;
  schema?: string | null;
  entityType: string;
  parent?: string | null;
}

export interface TableColumn {
  columnName: string;
  dataType: string;
  nullable: boolean;
  ordinalPosition: number;
}

export interface TableIndex {
  name: string;
  unique: boolean;
  primary: boolean;
  columns: string[];
}

export interface TableTrigger {
  name: string;
  timing?: string | null;
  manipulation?: string | null;
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

export interface RowChange {
  table: string;
  schema?: string | null;
  primaryKeys: [string, unknown][];
  values: [string, unknown][];
}

export interface TableChanges {
  inserts: RowChange[];
  updates: RowChange[];
  deletes: RowChange[];
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
