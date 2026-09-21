use std::collections::HashMap;
use std::path::PathBuf;

use graphite_appdb::{AppDb, SavedConnection, SavedQuery};
use graphite_core::{export_csv, export_json};
use graphite_core::{import_csv_rows, import_json_rows, import_xlsx_rows};
use graphite_core::{
    sql_dump, ConnectionConfig, DatabaseClient, NgQueryResult, QueryResult, Routine, SelectTop,
    TableChanges, TableColumn, TableIndex, TableOrView, TableTrigger,
};
use graphite_drivers::open_client;
use graphite_ssh::{open_tunnel, SshTunnel};
use serde::Serialize;
use tauri::{Emitter, State};
use tokio::sync::Mutex;
use uuid::Uuid;

struct Session {
    client: Box<dyn DatabaseClient>,
    _tunnel: Option<SshTunnel>,
}

pub struct AppState {
    sessions: Mutex<HashMap<String, Session>>,
    appdb: AppDb,
}

impl AppState {
    fn new() -> Self {
        let path = AppDb::default_path();
        let appdb = AppDb::open(path).expect("open appdb");
        Self {
            sessions: Mutex::new(HashMap::new()),
            appdb,
        }
    }
}

fn session_client<'a>(
    sessions: &'a mut HashMap<String, Session>,
    session_id: &str,
) -> Result<&'a mut Box<dyn DatabaseClient>, String> {
    sessions
        .get_mut(session_id)
        .map(|s| &mut s.client)
        .ok_or_else(|| "session not found".into())
}

#[tauri::command]
async fn conn_create(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<String, String> {
    let mut config = config;
    let mut tunnel = None;
    if let Some(ssh) = config.ssh.clone() {
        let dest_host = config.host.clone().unwrap_or_else(|| "127.0.0.1".into());
        let dest_port = config.port.unwrap_or(5432);
        let opened = open_tunnel(&ssh, &dest_host, dest_port)
            .await
            .map_err(|e| e.to_string())?;
        config.host = Some("127.0.0.1".into());
        config.port = Some(opened.local_port);
        tunnel = Some(opened);
    }
    let mut client = open_client(config).await.map_err(|e| e.to_string())?;
    client.connect().await.map_err(|e| e.to_string())?;
    let id = Uuid::new_v4().to_string();
    state
        .sessions
        .lock()
        .await
        .insert(
            id.clone(),
            Session {
                client,
                _tunnel: tunnel,
            },
        );
    Ok(id)
}

#[tauri::command]
async fn conn_disconnect(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    let mut sessions = state.sessions.lock().await;
    if let Some(mut session) = sessions.remove(&session_id) {
        session.client.disconnect().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn conn_version_string(state: State<'_, AppState>, session_id: String) -> Result<String, String> {
    let mut sessions = state.sessions.lock().await;
    session_client(&mut sessions, &session_id)?
        .version_string()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn conn_list_tables(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<TableOrView>, String> {
    let mut sessions = state.sessions.lock().await;
    session_client(&mut sessions, &session_id)?
        .list_tables()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn conn_list_views(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<TableOrView>, String> {
    let mut sessions = state.sessions.lock().await;
    session_client(&mut sessions, &session_id)?
        .list_views()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn conn_list_table_columns(
    state: State<'_, AppState>,
    session_id: String,
    table: String,
    schema: Option<String>,
) -> Result<Vec<TableColumn>, String> {
    let mut sessions = state.sessions.lock().await;
    session_client(&mut sessions, &session_id)?
        .list_table_columns(&table, schema.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn conn_list_databases(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<String>, String> {
    let mut sessions = state.sessions.lock().await;
    session_client(&mut sessions, &session_id)?
        .list_databases()
        .await
        .map_err(|e| e.to_string())
}

async fn execute_sql(
    state: &State<'_, AppState>,
    session_id: &str,
    sql: &str,
) -> Result<Vec<QueryResult>, String> {
    let _ = state.appdb.add_history(sql);
    let mut sessions = state.sessions.lock().await;
    session_client(&mut sessions, session_id)?
        .execute_query(sql)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn query_execute(
    state: State<'_, AppState>,
    session_id: String,
    sql: Option<String>,
    query_text: Option<String>,
) -> Result<Vec<NgQueryResult>, String> {
    let sql = sql
        .or(query_text)
        .ok_or_else(|| "sql missing".to_string())?;
    Ok(execute_sql(&state, &session_id, &sql)
        .await?
        .into_iter()
        .map(NgQueryResult::from)
        .collect())
}

#[tauri::command]
async fn conn_select_top(
    state: State<'_, AppState>,
    session_id: String,
    opts: SelectTop,
) -> Result<NgQueryResult, String> {
    let mut sessions = state.sessions.lock().await;
    let result = session_client(&mut sessions, &session_id)?
        .select_top(opts)
        .await
        .map_err(|e| e.to_string())?;
    Ok(NgQueryResult::from(result))
}

#[tauri::command]
async fn conn_connect() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
async fn conn_list_schemas(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<String>, String> {
    let mut sessions = state.sessions.lock().await;
    session_client(&mut sessions, &session_id)?
        .list_schemas()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn conn_list_routines(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<Routine>, String> {
    let mut sessions = state.sessions.lock().await;
    session_client(&mut sessions, &session_id)?
        .list_routines()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn conn_list_table_indexes(
    state: State<'_, AppState>,
    session_id: String,
    table: String,
    schema: Option<String>,
) -> Result<Vec<TableIndex>, String> {
    let mut sessions = state.sessions.lock().await;
    session_client(&mut sessions, &session_id)?
        .list_table_indexes(&table, schema.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn conn_list_table_triggers(
    state: State<'_, AppState>,
    session_id: String,
    table: String,
    schema: Option<String>,
) -> Result<Vec<TableTrigger>, String> {
    let mut sessions = state.sessions.lock().await;
    session_client(&mut sessions, &session_id)?
        .list_table_triggers(&table, schema.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn conn_default_schema(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Option<String>, String> {
    let mut sessions = state.sessions.lock().await;
    session_client(&mut sessions, &session_id)?
        .default_schema()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn conn_apply_changes(
    state: State<'_, AppState>,
    session_id: String,
    changes: TableChanges,
) -> Result<u64, String> {
    let mut sessions = state.sessions.lock().await;
    session_client(&mut sessions, &session_id)?
        .apply_changes(changes)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn conn_get_primary_keys(
    state: State<'_, AppState>,
    session_id: String,
    table: String,
    schema: Option<String>,
) -> Result<Vec<String>, String> {
    let mut sessions = state.sessions.lock().await;
    session_client(&mut sessions, &session_id)?
        .get_primary_keys(&table, schema.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn conn_supported_features() -> Result<graphite_core::SupportedFeatures, String> {
    Ok(graphite_core::SupportedFeatures::all())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportOk {
    path: String,
}

#[tauri::command]
async fn export_result(
    result: QueryResult,
    path: String,
    format: String,
) -> Result<ExportOk, String> {
    let path = PathBuf::from(path);
    match format.as_str() {
        "json" => export_json(&path, &result).map_err(|e| e.to_string())?,
        _ => export_csv(&path, &result).map_err(|e| e.to_string())?,
    }
    Ok(ExportOk {
        path: path.to_string_lossy().into(),
    })
}

#[tauri::command]
async fn query_execute_to_file(
    state: State<'_, AppState>,
    session_id: String,
    sql: String,
    path: String,
    format: String,
) -> Result<ExportOk, String> {
    let results = execute_sql(&state, &session_id, &sql).await?;
    let result = results.into_iter().next().unwrap_or_default();
    export_result(result, path, format).await
}

#[tauri::command]
async fn import_file(path: String, format: String) -> Result<graphite_core::ImportedTable, String> {
    let path = PathBuf::from(path);
    match format.as_str() {
        "json" => import_json_rows(&path).map_err(|e| e.to_string()),
        "xlsx" => import_xlsx_rows(&path).map_err(|e| e.to_string()),
        _ => import_csv_rows(&path).map_err(|e| e.to_string()),
    }
}

#[tauri::command]
async fn backup_table(
    state: State<'_, AppState>,
    session_id: String,
    table: String,
    path: String,
) -> Result<ExportOk, String> {
    let results = {
        let mut sessions = state.sessions.lock().await;
        session_client(&mut sessions, &session_id)?
            .execute_query(&format!("select * from {table}"))
            .await
            .map_err(|e| e.to_string())?
    };
    let result = results.into_iter().next().unwrap_or_default();
    let dump = sql_dump(&table, &result).map_err(|e| e.to_string())?;
    std::fs::write(&path, dump).map_err(|e| e.to_string())?;
    Ok(ExportOk { path })
}

#[tauri::command]
fn appdb_saved_find(state: State<'_, AppState>) -> Result<Vec<SavedConnection>, String> {
    state.appdb.list_connections().map_err(|e| e.to_string())
}

#[tauri::command]
fn appdb_saved_save(
    state: State<'_, AppState>,
    obj: SavedConnection,
) -> Result<SavedConnection, String> {
    state.appdb.save_connection(obj).map_err(|e| e.to_string())
}

#[tauri::command]
fn appdb_saved_remove(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.appdb.remove_connection(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn appdb_query_find(state: State<'_, AppState>) -> Result<Vec<SavedQuery>, String> {
    state.appdb.list_queries().map_err(|e| e.to_string())
}

#[tauri::command]
fn appdb_query_save(state: State<'_, AppState>, obj: SavedQuery) -> Result<SavedQuery, String> {
    state.appdb.save_query(obj).map_err(|e| e.to_string())
}

#[tauri::command]
fn appdb_setting_get(state: State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    state.appdb.get_setting(&key).map_err(|e| e.to_string())
}

#[tauri::command]
fn appdb_setting_set(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    state.appdb.set_setting(&key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
fn appdb_history_find(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    state.appdb.list_history().map_err(|e| e.to_string())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState::new())
        .setup(|app| {
            use tauri::Manager;
            use tauri::menu::{MenuBuilder, SubmenuBuilder};
            if let Some(win) = app.get_webview_window("main") {
                let win = win.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let _ = win.unminimize();
                    let _ = win.set_size(tauri::LogicalSize::new(1280.0, 800.0));
                    let _ = win.center();
                    let _ = win.show();
                    let _ = win.set_focus();
                });
            }
            let file_menu = SubmenuBuilder::new(app, "Arquivo")
                .text("open-sqlite", "Abrir SQLite…")
                .separator()
                .quit()
                .build()?;
            let edit_menu = SubmenuBuilder::new(app, "Editar")
                .copy()
                .paste()
                .select_all()
                .build()?;
            let menu = MenuBuilder::new(app)
                .item(&file_menu)
                .item(&edit_menu)
                .build()?;
            app.set_menu(menu)?;
            Ok(())
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "open-sqlite" {
                let _ = app.emit("menu://open-sqlite", ());
            }
        })
        .invoke_handler(tauri::generate_handler![
            conn_create,
            conn_connect,
            conn_disconnect,
            conn_version_string,
            conn_default_schema,
            conn_list_tables,
            conn_list_views,
            conn_list_table_columns,
            conn_list_databases,
            conn_list_schemas,
            conn_list_routines,
            conn_list_table_indexes,
            conn_list_table_triggers,
            query_execute,
            conn_select_top,
            conn_apply_changes,
            conn_get_primary_keys,
            conn_supported_features,
            export_result,
            query_execute_to_file,
            import_file,
            backup_table,
            appdb_saved_find,
            appdb_saved_save,
            appdb_saved_remove,
            appdb_query_find,
            appdb_query_save,
            appdb_setting_get,
            appdb_setting_set,
            appdb_history_find
        ])
        .run(tauri::generate_context!())
        .expect("error while running Graphite");
}
