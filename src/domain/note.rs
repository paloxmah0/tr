use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "note_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum NoteStatus {
    Pending,
    Extracted,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: Uuid,
    pub account_id: Uuid,
    pub title: String,
    pub content: String,
    pub content_type: String, // "text", "markdown", "json", "yaml"
    pub status: NoteStatus,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub processed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateNote {
    pub title: String,
    pub content: String,
    #[serde(default = "default_content_type")]
    pub content_type: String,
}

fn default_content_type() -> String { "markdown".into() }
