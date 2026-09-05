use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct StarterTemplate {
    name: String,
    files: Vec<StarterFile>,
}

#[derive(Deserialize, Serialize)]
struct StarterFile {
    name: String,
    source: String,
}

fn validate(template: &StarterTemplate) -> AppResult<()> {
    let unique_names = template
        .files
        .iter()
        .map(|file| file.name.as_str())
        .collect::<std::collections::HashSet<_>>();
    if template.name.trim().is_empty()
        || template.files.is_empty()
        || unique_names.len() != template.files.len()
        || !template.files.iter().any(|file| file.name == "main.lua")
        || template
            .files
            .iter()
            .any(|file| file.name.trim().is_empty())
    {
        return Err(AppError::status(
            StatusCode::BAD_REQUEST,
            "invalid starter template",
        ));
    }
    Ok(())
}

async fn get_starter(State(state): State<AppState>) -> AppResult<Json<StarterTemplate>> {
    let (value,): (String,) =
        sqlx::query_as("SELECT value FROM settings WHERE key='starter_template'")
            .fetch_one(&state.pool)
            .await?;
    Ok(Json(serde_json::from_str(&value)?))
}

async fn put_starter(
    State(state): State<AppState>,
    user: AuthUser,
    Json(template): Json<StarterTemplate>,
) -> AppResult<StatusCode> {
    if !user.is_admin {
        return Err(AppError::status(StatusCode::FORBIDDEN, "admin only"));
    }
    validate(&template)?;
    sqlx::query("UPDATE settings SET value=? WHERE key='starter_template'")
        .bind(serde_json::to_string(&template)?)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/starter", get(get_starter).put(put_starter))
}
