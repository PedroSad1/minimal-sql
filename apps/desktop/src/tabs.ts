import type { NgQueryResult, TableColumn, TableIndex, TableOrView, TableTrigger } from "./ipc";

export type TabKind = "query" | "table" | "structure";

export interface StructureView {
  title: string;
  columns: TableColumn[];
  indexes: TableIndex[];
  triggers: TableTrigger[];
}

export interface TabConnection {
  id: string | null;
  name: string;
}

export interface WorkspaceTab {
  id: string;
  kind: TabKind;
  title: string;
  sql: string;
  result: NgQueryResult;
  error: string;
  busy: boolean;
  table: TableOrView | null;
  primaryKeys: string[];
  structure: StructureView | null;
  connectionId: string | null;
  connectionName: string;
  tableFilter: TableFilterDraft[] | null;
  resultMode: "replace" | "append";
  loadGen: number;
}

export interface TableFilterDraft {
  field: string;
  op: string;
  value: string;
  join: "and" | "or";
}

export function emptyResult(): NgQueryResult {
  return { fields: [], rows: [], rowCount: 0 };
}

export function tableTabKey(table: TableOrView) {
  return `${table.schema || ""}.${table.name}`;
}

function withConnection(connection?: TabConnection) {
  return {
    connectionId: connection?.id ?? null,
    connectionName: connection?.name ?? "",
  };
}

export function createQueryTab(title: string, sql = "", connection?: TabConnection): WorkspaceTab {
  return {
    id: crypto.randomUUID(),
    kind: "query",
    title,
    sql,
    result: emptyResult(),
    error: "",
    busy: false,
    table: null,
    primaryKeys: [],
    structure: null,
    tableFilter: null,
    resultMode: "replace",
    loadGen: 0,
    ...withConnection(connection),
  };
}

export function createTableTab(table: TableOrView, connection?: TabConnection): WorkspaceTab {
  return {
    id: crypto.randomUUID(),
    kind: "table",
    title: table.name,
    sql: "",
    result: emptyResult(),
    error: "",
    busy: false,
    table,
    primaryKeys: [],
    structure: null,
    tableFilter: null,
    resultMode: "replace",
    loadGen: 0,
    ...withConnection(connection),
  };
}

export function createStructureTab(table: TableOrView, connection?: TabConnection): WorkspaceTab {
  return {
    id: crypto.randomUUID(),
    kind: "structure",
    title: table.name,
    sql: "",
    result: emptyResult(),
    error: "",
    busy: false,
    table,
    primaryKeys: [],
    structure: null,
    tableFilter: null,
    resultMode: "replace",
    loadGen: 0,
    ...withConnection(connection),
  };
}
