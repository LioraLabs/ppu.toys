use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};

fn require_admin(u: &AuthUser) -> AppResult<()> {
    if u.is_admin {
        Ok(())
    } else {
        Err(AppError::status(StatusCode::FORBIDDEN, "admin only"))
    }
}

async fn delete_toy(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Response> {
    require_admin(&user)?;
    // One transaction: detach forks first (forked_from is RESTRICT, so a toy others
    // forked would otherwise FK-fail the delete), then delete the toy. The hearts /
    // toy_sources / toy_revisions rows go via ON DELETE CASCADE.
    let mut tx = s.pool.begin().await?;
    sqlx::query("UPDATE toys SET forked_from=NULL WHERE forked_from=?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM toys WHERE id=?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(serde::Deserialize)]
struct BanBody {
    discord_id: String,
}

async fn ban(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<BanBody>,
) -> AppResult<Response> {
    require_admin(&user)?;
    let now = crate::db::now();
    sqlx::query("INSERT OR IGNORE INTO bans(discord_id,created_at) VALUES(?,?)")
        .bind(&b.discord_id)
        .bind(now)
        .execute(&s.pool)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id=?")
        .bind(&b.discord_id)
        .execute(&s.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn unban(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Response> {
    require_admin(&user)?;
    sqlx::query("DELETE FROM bans WHERE discord_id=?")
        .bind(id)
        .execute(&s.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(serde::Serialize, sqlx::FromRow)]
struct AdminToy {
    id: String,
    title: String,
    state: String,
    author: String,
    created_at: i64,
}

#[derive(serde::Serialize, sqlx::FromRow)]
struct AdminUser {
    id: String,
    handle: String,
    is_admin: bool,
    banned: bool,
    created_at: i64,
}

async fn overview(State(s): State<AppState>, user: AuthUser) -> AppResult<Json<serde_json::Value>> {
    require_admin(&user)?;
    let toys = sqlx::query_as::<_, AdminToy>(
        "SELECT t.id,t.title,t.state,u.handle author,t.created_at FROM toys t JOIN users u ON u.id=t.author_id ORDER BY t.created_at DESC LIMIT 200",
    )
    .fetch_all(&s.pool)
    .await?;
    let users = sqlx::query_as::<_, AdminUser>(
        "SELECT u.id,u.handle,u.is_admin != 0 is_admin,EXISTS(SELECT 1 FROM bans b WHERE b.discord_id=u.id) banned,u.created_at FROM users u ORDER BY u.created_at DESC LIMIT 200",
    )
    .fetch_all(&s.pool)
    .await?;
    Ok(Json(serde_json::json!({ "toys": toys, "users": users })))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin", get(overview))
        .route("/admin/toys/{id}", delete(delete_toy))
        .route("/admin/ban", post(ban))
        .route("/admin/ban/{id}", delete(unban))
}
