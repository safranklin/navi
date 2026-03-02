//! # Session Persistence
//!
//! Save/load conversations to `~/.navi/sessions/`.
//!
//! Each session is a JSON file (`<uuid>.json`) plus a lightweight index
//! (`sessions.json`) that avoids loading all files just to render a list.
//!
//! All writes use atomic rename (write `.tmp`, then `rename()`) for crash safety.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::Utc;
use log::{debug, warn};
use serde::{Deserialize, Serialize};

use crate::core::state::App;
use crate::inference::{ContextItem, Source};

/// Summary metadata for a session (stored in the index file).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: usize,
    pub model_name: String,
}

/// Full session data: metadata + conversation items.
#[derive(Serialize, Deserialize, Debug)]
pub struct SessionData {
    pub meta: SessionMeta,
    pub items: Vec<ContextItem>,
}

/// Index of all sessions, sorted by file mtime descending (most recently opened first).
#[derive(Serialize, Deserialize, Default, Debug)]
pub struct SessionIndex {
    pub sessions: Vec<SessionMeta>,
}

/// Returns `~/.navi/sessions/`, creating it if needed.
pub fn sessions_dir() -> io::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home directory"))?;
    let dir = home.join(".navi").join("sessions");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Generate a new UUID v4 session ID.
pub fn new_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Derive a title from the first user message in the conversation.
/// Returns the first line, truncated to 60 chars.
pub fn derive_title(items: &[ContextItem]) -> String {
    for item in items {
        if let ContextItem::Message(seg) = item
            && seg.source == Source::User
        {
            let first_line = seg.content.lines().next().unwrap_or("").trim();
            if first_line.len() > 60 {
                return format!("{}...", &first_line[..57]);
            }
            return first_line.to_string();
        }
    }
    "Untitled".to_string()
}

/// Count user + model messages (not directives, status, tool calls, etc.).
fn count_messages(items: &[ContextItem]) -> usize {
    items
        .iter()
        .filter(|item| {
            matches!(item, ContextItem::Message(seg) if matches!(seg.source, Source::User | Source::Model))
        })
        .count()
}

/// Filter out Directive and Status items (they're recreated on load).
fn persistable_items(items: &[ContextItem]) -> Vec<ContextItem> {
    items
        .iter()
        .filter(|item| {
            !matches!(item, ContextItem::Message(seg) if matches!(seg.source, Source::Directive | Source::Status))
        })
        .cloned()
        .collect()
}

/// Touch a file to update its mtime to now.
fn touch(path: &Path) -> io::Result<()> {
    let file = fs::OpenOptions::new().write(true).open(path)?;
    file.set_modified(SystemTime::now())?;
    Ok(())
}

/// Get a session file's mtime as a sortable value (descending = most recent first).
fn session_mtime(dir: &Path, id: &str) -> SystemTime {
    let path = dir.join(format!("{}.json", id));
    fs::metadata(&path)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Sort index entries by their session file's mtime, most recent first.
fn sort_index_by_mtime(index: &mut SessionIndex, dir: &Path) {
    index.sessions.sort_by(|a, b| {
        let ma = session_mtime(dir, &a.id);
        let mb = session_mtime(dir, &b.id);
        mb.cmp(&ma)
    });
}

/// Atomically write `data` as JSON to `path` (via `.tmp` + rename).
fn atomic_write_json<T: Serialize>(path: &Path, data: &T) -> io::Result<()> {
    let tmp_path = path.with_extension("tmp");
    let json = serde_json::to_string_pretty(data)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp_path, json)?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Save a session to disk and update the index.
pub fn save_session(
    id: &str,
    items: &[ContextItem],
    model_name: &str,
    existing_meta: Option<&SessionMeta>,
) -> io::Result<()> {
    let dir = sessions_dir()?;
    let now = Utc::now().timestamp();

    let persistable = persistable_items(items);
    let message_count = count_messages(items);

    // Don't save empty sessions (no user/model messages)
    if message_count == 0 {
        return Ok(());
    }

    let meta = SessionMeta {
        id: id.to_string(),
        title: existing_meta
            .map(|m| m.title.clone())
            .unwrap_or_else(|| derive_title(items)),
        created_at: existing_meta.map(|m| m.created_at).unwrap_or(now),
        updated_at: now,
        message_count,
        model_name: model_name.to_string(),
    };

    let data = SessionData {
        meta: meta.clone(),
        items: persistable,
    };

    // Write session file
    let session_path = dir.join(format!("{}.json", id));
    atomic_write_json(&session_path, &data)?;

    // Update index, sorted by file mtime (most recently touched first)
    let mut index = load_index().unwrap_or_default();
    index.sessions.retain(|s| s.id != id);
    index.sessions.push(meta);
    sort_index_by_mtime(&mut index, &dir);

    let index_path = dir.join("sessions.json");
    atomic_write_json(&index_path, &index)?;

    Ok(())
}

/// Load a session from disk by ID.
/// Touches the file to update mtime so it sorts as most-recently-opened.
pub fn load_session(id: &str) -> io::Result<SessionData> {
    let dir = sessions_dir()?;
    let path = dir.join(format!("{}.json", id));
    let json = fs::read_to_string(&path)?;
    let data: SessionData =
        serde_json::from_str(&json).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    // Touch file so mtime reflects "last opened", not just "last saved"
    let _ = touch(&path);

    Ok(data)
}

/// Delete a session from disk and update the index.
pub fn delete_session(id: &str) -> io::Result<()> {
    let dir = sessions_dir()?;
    let path = dir.join(format!("{}.json", id));
    if path.exists() {
        fs::remove_file(path)?;
    }

    let mut index = load_index().unwrap_or_default();
    index.sessions.retain(|s| s.id != id);
    let index_path = dir.join("sessions.json");
    atomic_write_json(&index_path, &index)?;

    Ok(())
}

/// Load the session index from disk.
pub fn load_index() -> io::Result<SessionIndex> {
    let dir = sessions_dir()?;
    let path = dir.join("sessions.json");
    if !path.exists() {
        return Ok(SessionIndex::default());
    }
    let json = fs::read_to_string(path)?;
    serde_json::from_str(&json).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Save the current app session to disk. Generates a session ID if needed.
/// Skips empty sessions (no user/model messages). This is the single entry
/// point for session persistence — call from the TUI on SaveSession effect or quit.
pub fn save_current_session(app: &mut App) {
    let has_messages = app.context.items.iter().any(|item| {
        matches!(item, ContextItem::Message(seg) if matches!(seg.source, Source::User | Source::Model))
    });
    if !has_messages {
        return;
    }

    let id = app
        .current_session_id
        .get_or_insert_with(new_session_id)
        .clone();

    // Load existing meta to preserve title/created_at
    let existing_meta = load_session(&id).ok().map(|d| d.meta);

    if let Err(e) = save_session(&id, &app.context.items, &app.model_name, existing_meta.as_ref())
    {
        warn!("Failed to save session: {}", e);
    } else {
        debug!("Session saved: {}", id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::ContextSegment;

    fn user_msg(text: &str) -> ContextItem {
        ContextItem::Message(ContextSegment {
            source: Source::User,
            content: text.to_string(),
        })
    }

    fn model_msg(text: &str) -> ContextItem {
        ContextItem::Message(ContextSegment {
            source: Source::Model,
            content: text.to_string(),
        })
    }

    fn directive_msg() -> ContextItem {
        ContextItem::Message(ContextSegment {
            source: Source::Directive,
            content: "system prompt".to_string(),
        })
    }

    #[test]
    fn test_derive_title_from_first_user_message() {
        let items = vec![directive_msg(), user_msg("What is Rust?"), model_msg("Rust is...")];
        assert_eq!(derive_title(&items), "What is Rust?");
    }

    #[test]
    fn test_derive_title_truncates_long_messages() {
        let long = "a".repeat(80);
        let items = vec![user_msg(&long)];
        let title = derive_title(&items);
        assert!(title.len() <= 63); // 57 + "..."
        assert!(title.ends_with("..."));
    }

    #[test]
    fn test_derive_title_uses_first_line() {
        let items = vec![user_msg("First line\nSecond line\nThird line")];
        assert_eq!(derive_title(&items), "First line");
    }

    #[test]
    fn test_derive_title_no_user_messages() {
        let items = vec![directive_msg()];
        assert_eq!(derive_title(&items), "Untitled");
    }

    #[test]
    fn test_persistable_items_filters_directive_and_status() {
        let items = vec![
            directive_msg(),
            user_msg("hello"),
            ContextItem::Message(ContextSegment {
                source: Source::Status,
                content: "Loading...".to_string(),
            }),
            model_msg("hi"),
        ];
        let filtered = persistable_items(&items);
        assert_eq!(filtered.len(), 2);
        assert!(matches!(&filtered[0], ContextItem::Message(seg) if seg.source == Source::User));
        assert!(matches!(&filtered[1], ContextItem::Message(seg) if seg.source == Source::Model));
    }

    #[test]
    fn test_count_messages() {
        let items = vec![
            directive_msg(),
            user_msg("hello"),
            model_msg("hi"),
            user_msg("how are you"),
        ];
        assert_eq!(count_messages(&items), 3); // 2 user + 1 model
    }
}
