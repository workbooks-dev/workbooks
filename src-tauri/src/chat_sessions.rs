use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub messages: Vec<ChatMessage>,
}

fn get_db_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to get home directory")?;
    let workbooks_dir = home.join(".workbooks");

    if !workbooks_dir.exists() {
        std::fs::create_dir_all(&workbooks_dir)?;
    }

    Ok(workbooks_dir.join("chat_sessions.db"))
}

fn init_db() -> Result<Connection> {
    let db_path = get_db_path()?;
    let conn = Connection::open(db_path)?;

    // Create sessions table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    )?;

    // Create messages table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
        )",
        [],
    )?;

    Ok(conn)
}

pub fn create_session(title: String) -> Result<ChatSession> {
    let conn = init_db()?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();

    conn.execute(
        "INSERT INTO sessions (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, title, now, now],
    )?;

    Ok(ChatSession {
        id,
        title,
        created_at: now,
        updated_at: now,
        messages: vec![],
    })
}

pub fn list_sessions() -> Result<Vec<ChatSession>> {
    let conn = init_db()?;

    let mut stmt = conn.prepare(
        "SELECT id, title, created_at, updated_at FROM sessions ORDER BY updated_at DESC"
    )?;

    let sessions = stmt.query_map([], |row| {
        Ok(ChatSession {
            id: row.get(0)?,
            title: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
            messages: vec![],
        })
    })?
    .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(sessions)
}

pub fn get_session(session_id: String) -> Result<ChatSession> {
    let conn = init_db()?;

    // Get session info
    let mut stmt = conn.prepare(
        "SELECT id, title, created_at, updated_at FROM sessions WHERE id = ?1"
    )?;

    let session = stmt.query_row(params![session_id], |row| {
        Ok(ChatSession {
            id: row.get(0)?,
            title: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
            messages: vec![],
        })
    })?;

    // Get messages
    let mut msg_stmt = conn.prepare(
        "SELECT role, content, timestamp FROM messages WHERE session_id = ?1 ORDER BY timestamp ASC"
    )?;

    let messages = msg_stmt.query_map(params![session_id], |row| {
        Ok(ChatMessage {
            role: row.get(0)?,
            content: row.get(1)?,
            timestamp: row.get(2)?,
        })
    })?
    .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(ChatSession {
        messages,
        ..session
    })
}

pub fn delete_session(session_id: String) -> Result<()> {
    let conn = init_db()?;

    conn.execute("DELETE FROM messages WHERE session_id = ?1", params![session_id])?;
    conn.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])?;

    Ok(())
}

pub fn add_message(session_id: String, role: String, content: String) -> Result<()> {
    let conn = init_db()?;
    let timestamp = chrono::Utc::now().timestamp();

    conn.execute(
        "INSERT INTO messages (session_id, role, content, timestamp) VALUES (?1, ?2, ?3, ?4)",
        params![session_id, role, content, timestamp],
    )?;

    // Update session timestamp
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
        params![now, session_id],
    )?;

    Ok(())
}

// Tauri commands
#[tauri::command]
pub fn create_chat_session(title: String) -> Result<ChatSession, String> {
    create_session(title).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_chat_sessions() -> Result<Vec<ChatSession>, String> {
    list_sessions().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_chat_session(session_id: String) -> Result<ChatSession, String> {
    get_session(session_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_chat_session(session_id: String) -> Result<(), String> {
    delete_session(session_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_message_to_session(session_id: String, role: String, content: String) -> Result<(), String> {
    add_message(session_id, role, content).map_err(|e| e.to_string())
}
