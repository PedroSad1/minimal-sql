use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use graphite_core::{GraphiteError, Result};

pub struct AppDb {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnection {
    pub id: String,
    pub name: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedQuery {
    pub id: String,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
}

impl AppDb {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| GraphiteError::msg(e.to_string()))?;
        }
        let conn = Connection::open(path).map_err(|e| GraphiteError::msg(e.to_string()))?;
        conn.execute_batch(
            "create table if not exists connections (
                id text primary key,
                name text not null,
                payload text not null
             );
             create table if not exists queries (
                id text primary key,
                title text not null,
                text text not null
             );
             create table if not exists settings (
                key text primary key,
                value text not null
             );
             create table if not exists history (
                id integer primary key autoincrement,
                sql text not null,
                created_at text not null
             );",
        )
        .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn default_path() -> PathBuf {
        dirs_path().join("graphite-app.db")
    }

    pub fn list_connections(&self) -> Result<Vec<SavedConnection>> {
        let conn = self.conn.lock().map_err(|e| GraphiteError::msg(e.to_string()))?;
        let mut stmt = conn
            .prepare("select id, name, payload from connections order by name")
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                let payload: String = row.get(2)?;
                Ok(SavedConnection {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
                })
            })
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| GraphiteError::msg(e.to_string()))
    }

    pub fn save_connection(&self, item: SavedConnection) -> Result<SavedConnection> {
        let conn = self.conn.lock().map_err(|e| GraphiteError::msg(e.to_string()))?;
        let payload = serde_json::to_string(&item.payload).map_err(anyhow::Error::from)?;
        conn.execute(
            "insert or replace into connections (id, name, payload) values (?1, ?2, ?3)",
            params![item.id, item.name, payload],
        )
        .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(item)
    }

    pub fn remove_connection(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| GraphiteError::msg(e.to_string()))?;
        conn.execute("delete from connections where id = ?1", params![id])
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(())
    }

    pub fn list_queries(&self) -> Result<Vec<SavedQuery>> {
        let conn = self.conn.lock().map_err(|e| GraphiteError::msg(e.to_string()))?;
        let mut stmt = conn
            .prepare("select id, title, text from queries order by title")
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(SavedQuery {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    text: row.get(2)?,
                })
            })
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| GraphiteError::msg(e.to_string()))
    }

    pub fn save_query(&self, item: SavedQuery) -> Result<SavedQuery> {
        let conn = self.conn.lock().map_err(|e| GraphiteError::msg(e.to_string()))?;
        conn.execute(
            "insert or replace into queries (id, title, text) values (?1, ?2, ?3)",
            params![item.id, item.title, item.text],
        )
        .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(item)
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().map_err(|e| GraphiteError::msg(e.to_string()))?;
        let mut stmt = conn
            .prepare("select value from settings where key = ?1")
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let value = stmt
            .query_row(params![key], |row| row.get(0))
            .optional()
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(value)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| GraphiteError::msg(e.to_string()))?;
        conn.execute(
            "insert or replace into settings (key, value) values (?1, ?2)",
            params![key, value],
        )
        .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(())
    }

    pub fn add_history(&self, sql: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| GraphiteError::msg(e.to_string()))?;
        conn.execute(
            "insert into history (sql, created_at) values (?1, datetime('now'))",
            params![sql],
        )
        .map_err(|e| GraphiteError::msg(e.to_string()))?;
        Ok(())
    }

    pub fn list_history(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().map_err(|e| GraphiteError::msg(e.to_string()))?;
        let mut stmt = conn
            .prepare("select sql from history order by id desc limit 50")
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| GraphiteError::msg(e.to_string()))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| GraphiteError::msg(e.to_string()))
    }
}

trait OptionalExt<T> {
    fn optional(self) -> rusqlite::Result<Option<T>>;
}

impl<T> OptionalExt<T> for rusqlite::Result<T> {
    fn optional(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

fn dirs_path() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("Library/Application Support/Graphite");
    }
    std::env::temp_dir().join("graphite")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn saves_connection_and_setting() {
        let dir = tempfile::tempdir().unwrap();
        let db = AppDb::open(dir.path().join("app.db")).unwrap();
        db.save_connection(SavedConnection {
            id: "1".into(),
            name: "local".into(),
            payload: json!({"connectionType": "sqlite"}),
        })
        .unwrap();
        db.set_setting("theme", "dark").unwrap();
        assert_eq!(db.list_connections().unwrap().len(), 1);
        assert_eq!(db.get_setting("theme").unwrap().as_deref(), Some("dark"));
    }
}
