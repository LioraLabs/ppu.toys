use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, post};
use axum::{Json, Router};
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

fn require_admin(u: &AuthUser) -> AppResult<()> {
    if u.is_admin { Ok(()) } else { Err(AppError::status(StatusCode::FORBIDDEN, "admin only")) }
}

async fn delete_toy(State(s): State<AppState>, user: AuthUser, Path(id): Path<String>) -> AppResult<Response> {
    require_admin(&user)?;
    // One transaction: detach forks first (forked_from is RESTRICT, so a toy others
    // forked would otherwise FK-fail the delete), then delete the toy. The hearts /
    // toy_sources / toy_revisions rows go via ON DELETE CASCADE.
    let mut tx = s.pool.begin().await?;
    sqlx::query("UPDATE toys SET forked_from=NULL WHERE forked_from=?").bind(&id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM toys WHERE id=?").bind(&id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(serde::Deserialize)]
struct BanBody { discord_id: String }

async fn ban(State(s): State<AppState>, user: AuthUser, Json(b): Json<BanBody>) -> AppResult<Response> {
    require_admin(&user)?;
    let now = crate::db::now();
    sqlx::query("INSERT OR IGNORE INTO bans(discord_id,created_at) VALUES(?,?)").bind(&b.discord_id).bind(now).execute(&s.pool).await?;
    sqlx::query("DELETE FROM sessions WHERE user_id=?").bind(&b.discord_id).execute(&s.pool).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/admin/toys/{id}", delete(delete_toy)).route("/admin/ban", post(ban))
}
