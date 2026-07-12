use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::put;
use axum::Router;
use crate::auth::AuthUser;
use crate::error::AppResult;
use crate::state::AppState;

async fn add(State(state): State<AppState>, user: AuthUser, Path(id): Path<String>) -> AppResult<Response> {
    let now = crate::db::now();
    sqlx::query("INSERT OR IGNORE INTO hearts(user_id,toy_id,created_at) VALUES(?,?,?)").bind(&user.id).bind(&id).bind(now).execute(&state.pool).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
async fn remove(State(state): State<AppState>, user: AuthUser, Path(id): Path<String>) -> AppResult<Response> {
    sqlx::query("DELETE FROM hearts WHERE user_id=? AND toy_id=?").bind(&user.id).bind(&id).execute(&state.pool).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
pub fn routes() -> Router<AppState> { Router::new().route("/toys/{id}/heart", put(add).delete(remove)) }
