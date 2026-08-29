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

#[derive(serde::Deserialize)]
struct FeaturedBody {
    toy_id: Option<String>,
}

#[derive(serde::Deserialize)]
struct FeaturedToysBody {
    toy_ids: Vec<String>,
}

async fn set_featured(
    State(s): State<AppState>,
    user: AuthUser,
    Json(body): Json<FeaturedBody>,
) -> AppResult<StatusCode> {
    require_admin(&user)?;
    if let Some(id) = &body.toy_id {
        let published =
            sqlx::query_as::<_, (i64,)>("SELECT 1 FROM toys WHERE id=? AND state='published'")
                .bind(id)
                .fetch_optional(&s.pool)
                .await?
                .is_some();
        if !published {
            return Err(AppError::status(
                StatusCode::BAD_REQUEST,
                "featured toy must be published",
            ));
        }
    }
    sqlx::query("UPDATE settings SET value=? WHERE key='featured_toy'")
        .bind(body.toy_id.unwrap_or_default())
        .execute(&s.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_featured_toys(
    State(s): State<AppState>,
    user: AuthUser,
    Json(body): Json<FeaturedToysBody>,
) -> AppResult<StatusCode> {
    require_admin(&user)?;
    let ids: Vec<_> = body
        .toy_ids
        .into_iter()
        .filter(|id| !id.is_empty())
        .collect();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    if ids.len() > 5 || unique.len() != ids.len() {
        return Err(AppError::status(
            StatusCode::BAD_REQUEST,
            "choose up to five unique toys",
        ));
    }
    for id in &ids {
        let published =
            sqlx::query_as::<_, (i64,)>("SELECT 1 FROM toys WHERE id=? AND state='published'")
                .bind(id)
                .fetch_optional(&s.pool)
                .await?
                .is_some();
        if !published {
            return Err(AppError::status(
                StatusCode::BAD_REQUEST,
                "featured toys must be published",
            ));
        }
    }
    sqlx::query("UPDATE settings SET value=? WHERE key='featured_toys'")
        .bind(serde_json::to_string(&ids)?)
        .execute(&s.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
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
    storage_bytes: i64,
}

async fn overview(State(s): State<AppState>, user: AuthUser) -> AppResult<Json<serde_json::Value>> {
    require_admin(&user)?;
    let toys = sqlx::query_as::<_, AdminToy>(
        "SELECT t.id,t.title,t.state,u.handle author,t.created_at FROM toys t JOIN users u ON u.id=t.author_id ORDER BY t.created_at DESC LIMIT 200",
    )
    .fetch_all(&s.pool)
    .await?;
    let featured_toys = sqlx::query_as::<_, AdminToy>(
        "SELECT t.id,t.title,t.state,u.handle author,t.created_at FROM toys t JOIN users u ON u.id=t.author_id WHERE t.state='published' ORDER BY t.title",
    )
    .fetch_all(&s.pool)
    .await?;
    let users = sqlx::query_as::<_, AdminUser>(
        "SELECT u.id,u.handle,u.is_admin != 0 is_admin,EXISTS(SELECT 1 FROM bans b WHERE b.discord_id=u.id) banned,u.created_at,COALESCE((SELECT SUM(length(title)+length(description)+length(files_json)+COALESCE(length(clip),0)+COALESCE(length(thumb),0)) FROM toys WHERE author_id=u.id),0)+COALESCE((SELECT SUM(length(s.name)+length(s.kind)+COALESCE(length(s.builtin_id),0)+COALESCE(length(s.options_json),0)+COALESCE(length(s.payload),0)+COALESCE(length(s.meta_json),0)) FROM toy_sources s JOIN toys t ON t.id=s.toy_id WHERE t.author_id=u.id),0) storage_bytes FROM users u ORDER BY u.created_at DESC LIMIT 200",
    )
    .fetch_all(&s.pool)
    .await?;
    let (used,): (i64,) = sqlx::query_as("SELECT (page_count - freelist_count) * page_size FROM pragma_page_count(), pragma_freelist_count(), pragma_page_size()")
        .fetch_one(&s.pool)
        .await?;
    Ok(Json(serde_json::json!({
        "toys": toys,
        "featuredToys": featured_toys,
        "users": users,
        "storage": {
            "usedBytes": used,
            "limitBytes": crate::config::MAX_APP_STORAGE,
            "warning": used * 10 >= crate::config::MAX_APP_STORAGE * 7
        },
        "featuredToyId": sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key='featured_toy'")
            .fetch_one(&s.pool).await?.0,
        "featuredToyIds": serde_json::from_str::<Vec<String>>(
            &sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key='featured_toys'")
                .fetch_one(&s.pool).await?.0
        )?
    })))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin", get(overview))
        .route("/admin/toys/{id}", delete(delete_toy))
        .route("/admin/ban", post(ban))
        .route("/admin/ban/{id}", delete(unban))
        .route("/admin/featured", post(set_featured))
        .route("/admin/featured-toys", post(set_featured_toys))
}
