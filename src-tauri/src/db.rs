use crate::models::{
    now_ms, AppSettings, Conversation, PerformanceSample, ServerPhase, ServerStatus, StoredMessage,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct Db {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl Db {
    pub fn open(path: &Path, data_dir: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Could not create the database folder: {e}"))?;
        }
        let conn = Connection::open(path)
            .map_err(|e| format!("Could not open Bernard's database: {e}"))?;
        let _mode: String = conn
            .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
            .map_err(|e| format!("Could not enable SQLite WAL mode: {e}"))?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| format!("Could not enable SQLite foreign keys: {e}"))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| format!("Could not configure SQLite timeout: {e}"))?;

        let db = Self {
            conn: Mutex::new(conn),
            path: path.to_path_buf(),
        };
        db.migrate()?;
        db.ensure_settings(data_dir)?;
        Ok(db)
    }

    fn migrate(&self) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                value TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
                content TEXT NOT NULL,
                image_paths TEXT NOT NULL DEFAULT '[]',
                created_at INTEGER NOT NULL,
                ttft_ms INTEGER,
                tokens_per_second REAL,
                prompt_tokens INTEGER,
                completion_tokens INTEGER,
                cache_tokens INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_messages_conversation
                ON messages(conversation_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_conversations_updated
                ON conversations(updated_at DESC);
            CREATE TABLE IF NOT EXISTS performance (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                value_ms REAL NOT NULL,
                detail TEXT,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS server_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                can_send INTEGER NOT NULL,
                input_tokens INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );
            "#,
        )
        .map_err(|e| format!("Could not initialize Bernard's database: {e}"))?;
        Ok(())
    }

    fn ensure_settings(&self, data_dir: &Path) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        let exists: Option<i64> = conn
            .query_row("SELECT id FROM settings WHERE id = 1", [], |row| row.get(0))
            .optional()
            .map_err(|e| format!("Could not read settings: {e}"))?;
        if exists.is_none() {
            let defaults = AppSettings::new(data_dir);
            let value = serde_json::to_string(&defaults)
                .map_err(|e| format!("Could not serialize default settings: {e}"))?;
            conn.execute(
                "INSERT INTO settings (id, value, updated_at) VALUES (1, ?1, ?2)",
                params![value, now_ms()],
            )
            .map_err(|e| format!("Could not save default settings: {e}"))?;
        }
        Ok(())
    }

    pub fn load_settings(&self) -> Result<AppSettings, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        let value: String = conn
            .query_row("SELECT value FROM settings WHERE id = 1", [], |row| {
                row.get(0)
            })
            .map_err(|e| format!("Could not load settings: {e}"))?;
        serde_json::from_str(&value).map_err(|e| format!("Stored settings are invalid: {e}"))
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), String> {
        settings.validate()?;
        let value = serde_json::to_string(settings)
            .map_err(|e| format!("Could not serialize settings: {e}"))?;
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        conn.execute(
            "UPDATE settings SET value = ?1, updated_at = ?2 WHERE id = 1",
            params![value, now_ms()],
        )
        .map_err(|e| format!("Could not save settings: {e}"))?;
        Ok(())
    }

    pub fn create_conversation(&self, title: &str) -> Result<Conversation, String> {
        let now = now_ms();
        let conversation = Conversation {
            id: format!("conv_{}", uuid::Uuid::new_v4()),
            title: sanitize_title(title),
            created_at: now,
            updated_at: now,
            message_count: 0,
        };
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        conn.execute(
            "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                conversation.id,
                conversation.title,
                conversation.created_at,
                conversation.updated_at
            ],
        )
        .map_err(|e| format!("Could not create the conversation: {e}"))?;
        Ok(conversation)
    }

    pub fn get_conversation(&self, id: &str) -> Result<Conversation, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        conn.query_row(
            "SELECT c.id, c.title, c.created_at, c.updated_at, COUNT(m.id)
             FROM conversations c LEFT JOIN messages m ON m.conversation_id = c.id
             WHERE c.id = ?1 GROUP BY c.id",
            params![id],
            row_to_conversation,
        )
        .optional()
        .map_err(|e| format!("Could not load the conversation: {e}"))?
        .ok_or_else(|| "That conversation no longer exists.".to_string())
    }

    pub fn list_conversations(&self, search: &str) -> Result<Vec<Conversation>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        let term = format!("%{}%", search.trim().replace('%', "\\%").replace('_', "\\_"));
        let mut statement = conn
            .prepare(
                "SELECT c.id, c.title, c.created_at, c.updated_at, COUNT(m.id)
                 FROM conversations c LEFT JOIN messages m ON m.conversation_id = c.id
                 WHERE c.title LIKE ?1 ESCAPE '\\'
                 GROUP BY c.id ORDER BY c.updated_at DESC LIMIT 200",
            )
            .map_err(|e| format!("Could not prepare the conversation list: {e}"))?;
        let rows = statement
            .query_map(params![term], row_to_conversation)
            .map_err(|e| format!("Could not read conversations: {e}"))?;
        let mut conversations = Vec::new();
        for row in rows {
            conversations.push(row.map_err(|e| format!("Could not read a conversation: {e}"))?);
        }
        Ok(conversations)
    }

    pub fn rename_conversation(&self, id: &str, title: &str) -> Result<(), String> {
        let title = sanitize_title(title);
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        let changed = conn
            .execute(
                "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
                params![title, now_ms(), id],
            )
            .map_err(|e| format!("Could not rename the conversation: {e}"))?;
        if changed == 0 {
            return Err("That conversation no longer exists.".to_string());
        }
        Ok(())
    }

    pub fn delete_conversation(&self, id: &str) -> Result<Vec<String>, String> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        let images = Self::images_for_conversation(&conn, id)?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("Could not start the delete: {e}"))?;
        tx.execute("DELETE FROM conversations WHERE id = ?1", params![id])
            .map_err(|e| format!("Could not delete the conversation: {e}"))?;
        tx.commit()
            .map_err(|e| format!("Could not finish the delete: {e}"))?;
        Ok(images)
    }

    fn images_for_conversation(conn: &Connection, id: &str) -> Result<Vec<String>, String> {
        let mut statement = conn
            .prepare("SELECT image_paths FROM messages WHERE conversation_id = ?1")
            .map_err(|e| format!("Could not read attachments: {e}"))?;
        let rows = statement
            .query_map(params![id], |row| row.get::<_, String>(0))
            .map_err(|e| format!("Could not read attachments: {e}"))?;
        let mut seen = HashSet::new();
        let mut images = Vec::new();
        for row in rows {
            let paths: Vec<String> = row
                .ok()
                .and_then(|value| serde_json::from_str(&value).ok())
                .unwrap_or_default();
            for path in paths {
                if seen.insert(path.clone()) {
                    images.push(path);
                }
            }
        }
        Ok(images)
    }

    pub fn messages(&self, conversation_id: &str) -> Result<Vec<StoredMessage>, String> {        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        let mut statement = conn
            .prepare(
                "SELECT id, conversation_id, role, content, image_paths, created_at, ttft_ms,
                        tokens_per_second, prompt_tokens, completion_tokens, cache_tokens
                 FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC, rowid ASC",
            )
            .map_err(|e| format!("Could not prepare message history: {e}"))?;
        let rows = statement
            .query_map(params![conversation_id], row_to_message)
            .map_err(|e| format!("Could not read message history: {e}"))?;
        let mut messages = Vec::new();
        for row in rows {
            messages.push(row.map_err(|e| format!("Could not read a message: {e}"))?);
        }
        Ok(messages)
    }

    pub fn insert_message(&self, message: &StoredMessage) -> Result<(), String> {
        let images = serde_json::to_string(&message.image_paths)
            .map_err(|e| format!("Could not serialize attachments: {e}"))?;
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("Could not start saving the message: {e}"))?;
        tx.execute(
            "INSERT INTO messages (
                id, conversation_id, role, content, image_paths, created_at, ttft_ms,
                tokens_per_second, prompt_tokens, completion_tokens, cache_tokens
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                message.id,
                message.conversation_id,
                message.role,
                message.content,
                images,
                message.created_at,
                message.ttft_ms,
                message.tokens_per_second,
                message.prompt_tokens,
                message.completion_tokens,
                message.cache_tokens,
            ],
        )
        .map_err(|e| format!("Could not save the message: {e}"))?;
        tx.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![message.created_at, message.conversation_id],
        )
        .map_err(|e| format!("Could not update the conversation: {e}"))?;
        tx.commit()
            .map_err(|e| format!("Could not finish saving the message: {e}"))?;
        Ok(())
    }

    pub fn maybe_set_title_from_message(&self, conversation_id: &str, content: &str) -> Result<(), String> {
        let title = title_from_message(content);
        if title.is_empty() {
            return Ok(());
        }
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        conn.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2
             WHERE id = ?3 AND (title = 'New chat' OR title = '')",
            params![title, now_ms(), conversation_id],
        )
        .map_err(|e| format!("Could not set the conversation title: {e}"))?;
        Ok(())
    }

    pub fn message_by_id(&self, id: &str) -> Result<StoredMessage, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        conn.query_row(
            "SELECT id, conversation_id, role, content, image_paths, created_at, ttft_ms,
                    tokens_per_second, prompt_tokens, completion_tokens, cache_tokens
             FROM messages WHERE id = ?1",
            params![id],
            row_to_message,
        )
        .optional()
        .map_err(|e| format!("Could not load the message: {e}"))?
        .ok_or_else(|| "That message no longer exists.".to_string())
    }

    pub fn delete_from_message(&self, id: &str) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        let message = conn
            .query_row(
                "SELECT conversation_id FROM messages WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| format!("Could not find the message: {e}"))?
            .ok_or_else(|| "That message no longer exists.".to_string())?;
        conn.execute(
            "DELETE FROM messages WHERE conversation_id = ?1 AND rowid >=
                (SELECT rowid FROM messages WHERE id = ?2)",
            params![message, id],
        )
        .map_err(|e| format!("Could not remove the later messages: {e}"))?;
        conn.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now_ms(), message],
        )
        .map_err(|e| format!("Could not update the conversation: {e}"))?;
        Ok(())
    }

    pub fn delete_after_message(&self, id: &str) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        let message = conn
            .query_row(
                "SELECT conversation_id FROM messages WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| format!("Could not find the message: {e}"))?
            .ok_or_else(|| "That message no longer exists.".to_string())?;
        conn.execute(
            "DELETE FROM messages WHERE conversation_id = ?1 AND rowid >
                (SELECT rowid FROM messages WHERE id = ?2)",
            params![message, id],
        )
        .map_err(|e| format!("Could not remove the later messages: {e}"))?;
        Ok(())
    }

    pub fn delete_trailing_assistant(&self, conversation_id: &str) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        conn.execute(
            "DELETE FROM messages WHERE conversation_id = ?1 AND role = 'assistant' AND rowid >
                (SELECT COALESCE(MAX(rowid), 0) FROM messages
                 WHERE conversation_id = ?1 AND role = 'user')",
            params![conversation_id],
        )
        .map_err(|e| format!("Could not remove the old answer: {e}"))?;
        Ok(())
    }

    pub fn update_message_content(&self, id: &str, content: &str) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        conn.execute(
            "UPDATE messages SET content = ?1 WHERE id = ?2 AND role = 'user'",
            params![content, id],
        )
        .map_err(|e| format!("Could not edit the message: {e}"))?;
        Ok(())
    }

    pub fn record_performance(
        &self,
        kind: &str,
        value_ms: f64,
        detail: Option<&str>,
    ) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        conn.execute(
            "INSERT INTO performance (kind, value_ms, detail, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![kind, value_ms, detail, now_ms()],
        )
        .map_err(|e| format!("Could not record a performance sample: {e}"))?;
        Ok(())
    }

    pub fn performance(&self, limit: u32) -> Result<Vec<PerformanceSample>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        let mut statement = conn
            .prepare(
                "SELECT id, kind, value_ms, detail, created_at FROM performance
                 ORDER BY id DESC LIMIT ?1",
            )
            .map_err(|e| format!("Could not prepare performance data: {e}"))?;
        let rows = statement
            .query_map(params![limit], |row| {
                Ok(PerformanceSample {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    value_ms: row.get(2)?,
                    detail: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(|e| format!("Could not read performance data: {e}"))?;
        let mut samples = Vec::new();
        for row in rows {
            samples.push(row.map_err(|e| format!("Could not read a performance sample: {e}"))?);
        }
        Ok(samples)
    }

    pub fn last_tokens_per_second(&self) -> Result<Option<f64>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        conn.query_row(
            "SELECT tokens_per_second FROM messages
             WHERE role = 'assistant' AND tokens_per_second IS NOT NULL
             ORDER BY created_at DESC, rowid DESC LIMIT 1",
            [],
            |row| row.get::<_, f64>(0),
        )
        .optional()
        .map_err(|e| format!("Could not read the last speed: {e}"))
    }

    pub fn all_image_paths(&self) -> Result<Vec<String>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        let mut statement = conn
            .prepare("SELECT image_paths FROM messages WHERE image_paths <> '[]'")
            .map_err(|e| format!("Could not read stored images: {e}"))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("Could not read stored images: {e}"))?;
        let mut seen = HashSet::new();
        let mut images = Vec::new();
        for row in rows {
            let paths: Vec<String> = row
                .ok()
                .and_then(|value| serde_json::from_str(&value).ok())
                .unwrap_or_default();
            for path in paths {
                if seen.insert(path.clone()) {
                    images.push(path);
                }
            }
        }
        Ok(images)
    }

    pub fn clear_all(&self) -> Result<Vec<String>, String> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        let images = {
            let mut statement = conn
                .prepare("SELECT image_paths FROM messages WHERE image_paths <> '[]'")
                .map_err(|e| format!("Could not read stored images: {e}"))?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| format!("Could not read stored images: {e}"))?;
            let mut seen = HashSet::new();
            let mut paths = Vec::new();
            for row in rows {
                let values: Vec<String> = row
                    .ok()
                    .and_then(|value| serde_json::from_str(&value).ok())
                    .unwrap_or_default();
                for path in values {
                    if seen.insert(path.clone()) {
                        paths.push(path);
                    }
                }
            }
            paths
        };
        let tx = conn
            .transaction()
            .map_err(|e| format!("Could not start clearing data: {e}"))?;
        tx.execute("DELETE FROM conversations", [])
            .map_err(|e| format!("Could not clear conversations: {e}"))?;
        tx.execute("DELETE FROM performance", [])
            .map_err(|e| format!("Could not clear performance data: {e}"))?;
        tx.commit()
            .map_err(|e| format!("Could not finish clearing data: {e}"))?;
        Ok(images)
    }

    pub fn save_server_snapshot(&self, can_send: bool, input_tokens: u64) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        conn.execute(
            "INSERT INTO server_state (id, can_send, input_tokens, created_at)
             VALUES (1, ?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET can_send = excluded.can_send,
             input_tokens = excluded.input_tokens, created_at = excluded.created_at",
            params![can_send as i64, input_tokens as i64, now_ms()],
        )
        .map_err(|e| format!("Could not save server state: {e}"))?;
        Ok(())
    }

    pub fn server_snapshot(&self) -> Result<Option<crate::models::ServerStatus>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "The database lock was poisoned.".to_string())?;
        let exists: Option<i64> = conn
            .query_row("SELECT id FROM server_state WHERE id = 1", [], |row| row.get(0))
            .optional()
            .map_err(|e| format!("Could not read server state: {e}"))?;
        if exists.is_none() {
            return Ok(None);
        }
        let mut status = ServerStatus::stopped();
        status.phase = ServerPhase::Stopped;
        Ok(Some(status))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn row_to_conversation(row: &rusqlite::Row<'_>) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: row.get(0)?,
        title: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        message_count: row.get(4)?,
    })
}

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredMessage> {
    let images: String = row.get(4)?;
    Ok(StoredMessage {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        role: row.get(2)?,
        content: row.get(3)?,
        image_paths: serde_json::from_str(&images).unwrap_or_default(),
        created_at: row.get(5)?,
        ttft_ms: row.get(6)?,
        tokens_per_second: row.get(7)?,
        prompt_tokens: row.get(8)?,
        completion_tokens: row.get(9)?,
        cache_tokens: row.get(10)?,
    })
}

fn sanitize_title(title: &str) -> String {
    let clean = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.is_empty() {
        "New chat".to_string()
    } else {
        clean.chars().take(80).collect()
    }
}

fn title_from_message(content: &str) -> String {
    let clean = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title: String = clean.chars().take(48).collect();
    if clean.chars().count() > 48 {
        title.push('…');
    }
    title
}

pub fn sanitize_image_paths(paths: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    paths
        .iter()
        .filter(|path| !path.trim().is_empty())
        .filter(|path| seen.insert((*path).clone()))
        .cloned()
        .collect()
}
