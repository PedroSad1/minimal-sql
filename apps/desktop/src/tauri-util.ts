import { invoke } from "@tauri-apps/api/core";

/** Community `$util.send('conn/listTables')` → Tauri `invoke('conn_list_tables')`. */

let sessionId: string | null = null;
let pendingQueryId = 1;
const pendingQueries = new Map<number, string>();

export function setSessionId(id: string | null) {
  sessionId = id;
}

export function getSessionId() {
  return sessionId;
}

const CHANNEL_TO_COMMAND: Record<string, string> = {
  "conn/create": "conn_create",
  "conn/connect": "conn_connect",
  "conn/disconnect": "conn_disconnect",
  "conn/versionString": "conn_version_string",
  "conn/defaultSchema": "conn_default_schema",
  "conn/supportedFeatures": "conn_supported_features",
  "conn/listTables": "conn_list_tables",
  "conn/listViews": "conn_list_views",
  "conn/listTableColumns": "conn_list_table_columns",
  "conn/listDatabases": "conn_list_databases",
  "conn/listSchemas": "conn_list_schemas",
  "conn/listRoutines": "conn_list_routines",
  "conn/listTableIndexes": "conn_list_table_indexes",
  "conn/listTableTriggers": "conn_list_table_triggers",
  "conn/executeQuery": "query_execute",
  "conn/selectTop": "conn_select_top",
  "conn/applyChanges": "conn_apply_changes",
  "conn/getPrimaryKeys": "conn_get_primary_keys",
  "conn/exportQuery": "query_execute_to_file",
  "conn/importFile": "import_file",
  "appdb/saved/find": "appdb_saved_find",
  "appdb/saved/save": "appdb_saved_save",
  "appdb/saved/remove": "appdb_saved_remove",
  "appdb/query/find": "appdb_query_find",
  "appdb/query/save": "appdb_query_save",
  "appdb/setting/get": "appdb_setting_get",
  "appdb/setting/set": "appdb_setting_set",
  "appdb/history/find": "appdb_history_find",
};

const EMPTY_LIST_CHANNELS = new Set([
  "conn/listCharsets",
  "conn/listCollations",
  "conn/listMaterializedViewColumns",
  "conn/getTableReferences",
  "conn/getTableKeys",
  "conn/listTablePartitions",
  "conn/getCompletions",
]);

const UNSUPPORTED_CHANNELS = new Set([
  "conn/exportStream",
  "backup/run",
  "backup/connection",
]);

function withSession(payload: Record<string, unknown>) {
  const body = { ...payload };
  if (sessionId && body.sessionId == null && body.session_id == null) {
    body.sessionId = sessionId;
    body.session_id = sessionId;
  }
  return body;
}

export async function send<T = unknown>(
  channel: string,
  payload: Record<string, unknown> = {},
): Promise<T> {
  if (UNSUPPORTED_CHANNELS.has(channel)) {
    throw new Error("não suportado");
  }
  if (EMPTY_LIST_CHANNELS.has(channel)) {
    return [] as T;
  }
  if (channel === "conn/query") {
    const id = pendingQueryId++;
    pendingQueries.set(id, String(payload.queryText ?? payload.sql ?? ""));
    return id as T;
  }
  if (channel === "query/execute") {
    const sql = pendingQueries.get(Number(payload.queryId)) ?? "";
    pendingQueries.delete(Number(payload.queryId));
    return invoke<T>("query_execute", withSession({ sql, queryText: sql }));
  }
  if (channel === "query/cancel") {
    pendingQueries.delete(Number(payload.queryId));
    return undefined as T;
  }

  const command = CHANNEL_TO_COMMAND[channel];
  if (!command) {
    throw new Error(`não suportado: ${channel}`);
  }

  if (command === "conn_create") {
    const config = payload.config ?? payload;
    return invoke<T>(command, { config });
  }
  if (command === "appdb_saved_save" || command === "appdb_query_save") {
    return invoke<T>(command, { obj: payload.obj ?? payload });
  }
  if (command === "appdb_saved_remove") {
    return invoke<T>(command, { id: payload.id });
  }
  if (command === "appdb_setting_get") {
    return invoke<T>(command, { key: payload.key });
  }
  if (command === "appdb_setting_set") {
    return invoke<T>(command, { key: payload.key, value: payload.value });
  }
  if (command === "query_execute") {
    const sql = payload.queryText ?? payload.sql;
    return invoke<T>(command, withSession({ sql, queryText: sql }));
  }
  if (command === "conn_select_top") {
    return invoke<T>(command, withSession({ opts: payload.opts ?? payload }));
  }
  if (command === "conn_apply_changes") {
    return invoke<T>(command, withSession({ changes: payload.changes ?? payload }));
  }
  if (command === "query_execute_to_file") {
    return invoke<T>(
      command,
      withSession({
        sql: payload.sql,
        path: payload.path,
        format: payload.format,
      }),
    );
  }
  if (command === "import_file") {
    return invoke<T>(command, { path: payload.path, format: payload.format });
  }
  if (command === "conn_supported_features" || command === "conn_connect") {
    return invoke<T>(command);
  }
  return invoke<T>(command, withSession(payload));
}

export const util = { send };
