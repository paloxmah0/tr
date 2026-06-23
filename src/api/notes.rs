use crate::domain::note::{CreateNote, Note};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct ProcessResult {
    pub note: Note,
    pub strategy_id: Option<Uuid>,
    pub error: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    Json(req): Json<CreateNote>,
) -> AppResult<Json<Note>> {
    Ok(Json(state.db.create_note(account_id, &req).await?))
}

pub async fn list(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> AppResult<Json<Vec<Note>>> {
    Ok(Json(state.db.list_notes(account_id).await?))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Note>> {
    state.db.get_note(id).await?.ok_or_else(|| AppError::NotFound("note".into())).map(Json)
}

/// Trigger LLM extraction for a note. Creates a strategy from the note content.
pub async fn process(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<ProcessResult>> {
    let note_before = state.db.get_note(id).await?.ok_or_else(|| AppError::NotFound("note".into()))?;
    let account_id = note_before.account_id;

    let result = state.ingest.process_note(id, account_id).await;
    let note_after = state.db.get_note(id).await?.unwrap_or(note_before);

    match result {
        Ok(strategy_id) => Ok(Json(ProcessResult { note: note_after, strategy_id: Some(strategy_id), error: None })),
        Err(e) => Ok(Json(ProcessResult { note: note_after, strategy_id: None, error: Some(e.to_string()) })),
    }
}
