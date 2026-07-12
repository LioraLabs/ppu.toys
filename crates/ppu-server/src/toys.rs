use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use base64::Engine;
use rand::Rng;
use serde::{Deserialize, Serialize};
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Serialize, Deserialize)]
pub struct FileDto { pub name: String, pub source: String }

#[derive(Serialize, Deserialize)]
pub struct SourceDto {
    pub name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub builtin_id: Option<String>,
    #[serde(default)] pub options: serde_json::Value,
    #[serde(default)] pub meta: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub payload: Option<String>, // base64
}

#[derive(Deserialize)]
pub struct SaveBody { pub title: String, #[serde(default)] pub description: String, pub files: Vec<FileDto>, #[serde(default)] pub sources: Vec<SourceDto> }

fn slug() -> String {
    const ABC: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut r = rand::thread_rng();
    (0..8).map(|_| ABC[r.gen_range(0..ABC.len())] as char).collect()
}
fn b64(bytes: &[u8]) -> String { base64::engine::general_purpose::STANDARD.encode(bytes) }
fn unb64(s: &str) -> AppResult<Vec<u8>> { base64::engine::general_purpose::STANDARD.decode(s).map_err(|_| AppError::status(StatusCode::BAD_REQUEST, "bad base64 payload")) }

fn validate_files(files: &[FileDto]) -> AppResult<()> {
    for f in files { if f.source.len() > crate::config::CAP_LUA_FILE { return Err(AppError::status(StatusCode::PAYLOAD_TOO_LARGE, "lua file too large")); } }
    Ok(())
}

/// Upsert source metadata rows, then push each payload through the blob layer so
/// PPU_BLOB_MODE (db|disk) is honored uniformly. The row is written first (payload
/// column NULL); blobs::store then fills it (db) or writes a file (disk).
async fn write_sources(state: &AppState, toy_id: &str, sources: &[SourceDto]) -> AppResult<()> {
    for s in sources {
        let payload = match &s.payload { Some(p) => Some(unb64(p)?), None => None };
        if let Some(ref p) = payload { if p.len() > crate::config::CAP_SOURCE_PAYLOAD { return Err(AppError::status(StatusCode::PAYLOAD_TOO_LARGE, "source payload too large")); } }
        sqlx::query("INSERT INTO toy_sources(toy_id,name,kind,builtin_id,options_json,payload,meta_json) VALUES(?,?,?,?,?,NULL,?)
                     ON CONFLICT(toy_id,name) DO UPDATE SET kind=excluded.kind, builtin_id=excluded.builtin_id, options_json=excluded.options_json, meta_json=excluded.meta_json, payload=NULL")
            .bind(toy_id).bind(&s.name).bind(&s.kind).bind(&s.builtin_id)
            .bind(s.options.to_string()).bind(s.meta.to_string())
            .execute(&state.pool).await?;
        if let Some(p) = payload {
            crate::blobs::store(state, crate::blobs::BlobKey::Source(toy_id, &s.name), &p).await?;
        }
    }
    Ok(())
}

async fn snapshot_revision(state: &AppState, toy_id: &str, files_json: &str) -> AppResult<()> {
    let now = crate::db::now();
    let (rev,): (i64,) = sqlx::query_as("SELECT COALESCE(MAX(rev),0)+1 FROM toy_revisions WHERE toy_id=?").bind(toy_id).fetch_one(&state.pool).await?;
    sqlx::query("INSERT INTO toy_revisions(toy_id,rev,files_json,saved_at) VALUES(?,?,?,?)").bind(toy_id).bind(rev).bind(files_json).bind(now).execute(&state.pool).await?;
    Ok(())
}

async fn create(State(state): State<AppState>, user: AuthUser, Json(body): Json<SaveBody>) -> AppResult<Response> {
    if !state.limiter.check_save(&user.id) { return Err(AppError::status(StatusCode::TOO_MANY_REQUESTS, "save rate limit")); }
    validate_files(&body.files)?;
    let id = slug();
    let files_json = serde_json::to_string(&body.files)?;
    let now = crate::db::now();
    sqlx::query("INSERT INTO toys(id,author_id,title,description,files_json,state,created_at) VALUES(?,?,?,?,?, 'draft', ?)")
        .bind(&id).bind(&user.id).bind(&body.title).bind(&body.description).bind(&files_json).bind(now).execute(&state.pool).await?;
    write_sources(&state, &id, &body.sources).await?;
    snapshot_revision(&state, &id, &files_json).await?;
    Ok(Json(serde_json::json!({ "id": id })).into_response())
}

async fn update(State(state): State<AppState>, user: AuthUser, Path(id): Path<String>, Json(body): Json<SaveBody>) -> AppResult<Response> {
    if !state.limiter.check_save(&user.id) { return Err(AppError::status(StatusCode::TOO_MANY_REQUESTS, "save rate limit")); }
    let author: Option<(String,)> = sqlx::query_as("SELECT author_id FROM toys WHERE id=?").bind(&id).fetch_optional(&state.pool).await?;
    let author = author.ok_or_else(|| AppError::status(StatusCode::NOT_FOUND, "no such toy"))?.0;
    if author != user.id { return Err(AppError::status(StatusCode::FORBIDDEN, "not your toy")); }
    validate_files(&body.files)?;
    let files_json = serde_json::to_string(&body.files)?;
    sqlx::query("UPDATE toys SET title=?, description=?, files_json=? WHERE id=?").bind(&body.title).bind(&body.description).bind(&files_json).bind(&id).execute(&state.pool).await?;
    write_sources(&state, &id, &body.sources).await?;
    snapshot_revision(&state, &id, &files_json).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn get_toy(State(state): State<AppState>, maybe: Option<AuthUser>, Path(id): Path<String>) -> AppResult<Response> {
    let row: Option<(String,String,String,String,Option<String>,i64,String,String,Option<String>)> = sqlx::query_as(
        "SELECT t.title,t.description,t.files_json,t.state,t.forked_from,t.heart_count,u.handle,u.id,u.avatar_hash
         FROM toys t JOIN users u ON u.id=t.author_id WHERE t.id=?").bind(&id).fetch_optional(&state.pool).await?;
    let (title,description,files_json,tstate,forked_from,heart_count,handle,author_id,avatar) =
        row.ok_or_else(|| AppError::status(StatusCode::NOT_FOUND, "no such toy"))?;
    let files: serde_json::Value = serde_json::from_str(&files_json)?;
    let src_rows: Vec<(String,String,Option<String>,Option<String>,Option<String>)> = sqlx::query_as(
        "SELECT name,kind,builtin_id,options_json,meta_json FROM toy_sources WHERE toy_id=?").bind(&id).fetch_all(&state.pool).await?;
    let mut sources = Vec::new();
    for (name,kind,bid,opts,meta) in src_rows {
        let payload = crate::blobs::load(&state, crate::blobs::BlobKey::Source(&id, &name)).await?;
        sources.push(serde_json::json!({
            "name": name, "kind": kind, "builtinId": bid,
            "options": opts.and_then(|o| serde_json::from_str::<serde_json::Value>(&o).ok()).unwrap_or(serde_json::Value::Null),
            "meta": meta.and_then(|m| serde_json::from_str::<serde_json::Value>(&m).ok()).unwrap_or(serde_json::Value::Null),
            "payload": payload.map(|p| b64(&p)),
        }));
    }
    let hearted = if let Some(u) = &maybe {
        sqlx::query_as::<_,(i64,)>("SELECT 1 FROM hearts WHERE user_id=? AND toy_id=?").bind(&u.id).bind(&id).fetch_optional(&state.pool).await?.is_some()
    } else { false };
    Ok(Json(serde_json::json!({
        "id": id, "title": title, "description": description, "state": tstate,
        "files": files, "sources": sources, "heartCount": heart_count, "hearted": hearted, "forkedFrom": forked_from,
        "author": { "id": author_id, "handle": handle, "avatar": avatar },
    })).into_response())
}

#[derive(Deserialize)]
pub struct WallQuery { #[serde(default)] sort: Option<String>, #[serde(default)] page: Option<i64> }

const PAGE_SIZE: i64 = 24;

fn wall_card(id: &str, title: &str, handle: &str, avatar: &Option<String>, heart_count: i64, hearted: bool) -> serde_json::Value {
    serde_json::json!({
        "id": id, "title": title,
        "author": { "handle": handle, "avatar": avatar },
        "thumbUrl": format!("/blobs/thumb/{id}"),
        "clipUrl": format!("/blobs/clip/{id}"),
        "heartCount": heart_count, "hearted": hearted,
    })
}

async fn wall(State(state): State<AppState>, maybe: Option<AuthUser>, Query(q): Query<WallQuery>) -> AppResult<Response> {
    let page = q.page.unwrap_or(0).max(0);
    let order = match q.sort.as_deref() { Some("popular") => "t.heart_count DESC, t.created_at DESC", _ => "t.created_at DESC" };
    let sql = format!("SELECT t.id,t.title,t.heart_count,u.handle,u.avatar_hash FROM toys t JOIN users u ON u.id=t.author_id
                       WHERE t.state='published' ORDER BY {order} LIMIT ? OFFSET ?");
    let rows: Vec<(String,String,i64,String,Option<String>)> = sqlx::query_as(&sql)
        .bind(PAGE_SIZE + 1).bind(page * PAGE_SIZE).fetch_all(&state.pool).await?;
    let uid = maybe.as_ref().map(|u| u.id.clone());
    let has_more = rows.len() as i64 > PAGE_SIZE;
    let mut cards = Vec::new();
    for (id,title,hc,handle,avatar) in rows.into_iter().take(PAGE_SIZE as usize) {
        let hearted = match &uid { Some(u) => sqlx::query_as::<_,(i64,)>("SELECT 1 FROM hearts WHERE user_id=? AND toy_id=?").bind(u).bind(&id).fetch_optional(&state.pool).await?.is_some(), None => false };
        cards.push(wall_card(&id,&title,&handle,&avatar,hc,hearted));
    }
    Ok(Json(serde_json::json!({ "toys": cards, "nextPage": if has_more { Some(page+1) } else { None } })).into_response())
}

async fn profile(State(state): State<AppState>, maybe: Option<AuthUser>, Path(handle): Path<String>) -> AppResult<Response> {
    let u: Option<(String,String,Option<String>)> = sqlx::query_as("SELECT id,handle,avatar_hash FROM users WHERE handle=?").bind(&handle).fetch_optional(&state.pool).await?;
    let (uid, handle, avatar) = u.ok_or_else(|| AppError::status(StatusCode::NOT_FOUND, "no such user"))?;
    let rows: Vec<(String,String,i64)> = sqlx::query_as("SELECT id,title,heart_count FROM toys WHERE author_id=? AND state='published' ORDER BY created_at DESC").bind(&uid).fetch_all(&state.pool).await?;
    let viewer = maybe.as_ref().map(|x| x.id.clone());
    let mut cards = Vec::new();
    for (id,title,hc) in rows {
        let hearted = match &viewer { Some(v) => sqlx::query_as::<_,(i64,)>("SELECT 1 FROM hearts WHERE user_id=? AND toy_id=?").bind(v).bind(&id).fetch_optional(&state.pool).await?.is_some(), None => false };
        cards.push(wall_card(&id,&title,&handle,&avatar,hc,hearted));
    }
    Ok(Json(serde_json::json!({ "user": { "handle": handle, "avatar": avatar }, "toys": cards })).into_response())
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/toys", get(wall).post(create))
        .route("/toys/{id}", get(get_toy).put(update))
        .route("/users/{handle}", get(profile))
}
