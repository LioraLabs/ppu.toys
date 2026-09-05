use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Serialize, Deserialize)]
pub struct FileDto {
    pub name: String,
    pub source: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")] // write side accepts `builtinId`, matching the read side
pub struct SourceDto {
    pub name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builtin_id: Option<String>,
    #[serde(default)]
    pub options: serde_json::Value,
    #[serde(default)]
    pub meta: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<String>, // base64
}

#[derive(Deserialize)]
pub struct SaveBody {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    pub files: Vec<FileDto>,
    #[serde(default)]
    pub sources: Vec<SourceDto>,
    #[serde(default, rename = "expectedRevision")]
    pub expected_revision: Option<i64>,
    #[serde(default, rename = "forkedFrom")]
    pub forked_from: Option<String>,
}

/// Tags are a small, normalized set of public slugs; omitted tags preserve an
/// existing toy when older clients save it.
fn normalize_tags(tags: &[String]) -> AppResult<Vec<String>> {
    if tags.len() > 5 {
        return Err(AppError::status(StatusCode::BAD_REQUEST, "at most 5 tags"));
    }
    let mut result = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_ascii_lowercase();
        if tag.is_empty()
            || tag.len() > 24
            || !tag
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            || tag.starts_with('-')
            || tag.ends_with('-')
        {
            return Err(AppError::status(
                StatusCode::BAD_REQUEST,
                "tags must be 1–24 letters, numbers or hyphens",
            ));
        }
        if !result.contains(&tag) {
            result.push(tag);
        }
    }
    Ok(result)
}

fn slug() -> String {
    const ABC: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut r = rand::thread_rng();
    (0..8)
        .map(|_| ABC[r.gen_range(0..ABC.len())] as char)
        .collect()
}
fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
fn unb64(s: &str) -> AppResult<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|_| AppError::status(StatusCode::BAD_REQUEST, "bad base64 payload"))
}

/// Validate a save BEFORE any row is written: per-file cap, per-source-payload cap,
/// and the aggregate ≤1MB toy-total cap. Doing it up front means a cap violation on a
/// later item can't leave a half-written toy behind.
fn validate_save(body: &SaveBody) -> AppResult<i64> {
    if body.title.trim().is_empty() || body.title.len() > crate::config::CAP_TITLE {
        return Err(AppError::status(StatusCode::BAD_REQUEST, "invalid title"));
    }
    if body.description.len() > crate::config::CAP_DESCRIPTION {
        return Err(AppError::status(
            StatusCode::BAD_REQUEST,
            "description too long",
        ));
    }
    if body.files.len() > crate::config::MAX_FILES
        || body.sources.len() > crate::config::MAX_SOURCES
    {
        return Err(AppError::status(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too many files or sources",
        ));
    }
    let mut file_names = HashSet::new();
    let mut source_names = HashSet::new();
    let mut total = body.title.len() + body.description.len();
    for f in &body.files {
        if f.name.trim().is_empty()
            || f.name.len() > crate::config::CAP_NAME
            || !file_names.insert(&f.name)
        {
            return Err(AppError::status(
                StatusCode::BAD_REQUEST,
                "invalid file name",
            ));
        }
        if f.source.len() > crate::config::CAP_LUA_FILE {
            return Err(AppError::status(
                StatusCode::PAYLOAD_TOO_LARGE,
                "lua file too large",
            ));
        }
        total += f.name.len() + f.source.len();
    }
    for s in &body.sources {
        if s.name.trim().is_empty()
            || s.name.len() > crate::config::CAP_NAME
            || s.kind.trim().is_empty()
            || s.kind.len() > crate::config::CAP_NAME
            || s.builtin_id
                .as_ref()
                .is_some_and(|id| id.len() > crate::config::CAP_NAME)
            || !source_names.insert(&s.name)
        {
            return Err(AppError::status(StatusCode::BAD_REQUEST, "invalid source"));
        }
        total +=
            s.name.len() + s.kind.len() + s.options.to_string().len() + s.meta.to_string().len();
        if let Some(p) = &s.payload {
            let bytes = unb64(p)?;
            if bytes.len() > crate::config::CAP_SOURCE_PAYLOAD {
                return Err(AppError::status(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "source payload too large",
                ));
            }
            total += bytes.len();
        }
    }
    if total > crate::config::CAP_TOY_TOTAL {
        return Err(AppError::status(
            StatusCode::PAYLOAD_TOO_LARGE,
            "toy exceeds total size cap",
        ));
    }
    Ok(total as i64)
}

pub(crate) async fn user_storage(state: &AppState, user_id: &str) -> AppResult<i64> {
    let (bytes,): (i64,) = sqlx::query_as(
        "SELECT COALESCE((SELECT SUM(length(title)+length(description)+length(tags_json)+length(files_json)+COALESCE(length(clip),0)+COALESCE(length(thumb),0)) FROM toys WHERE author_id=?),0)+COALESCE((SELECT SUM(length(s.name)+length(s.kind)+COALESCE(length(s.builtin_id),0)+COALESCE(length(s.options_json),0)+COALESCE(length(s.payload),0)+COALESCE(length(s.meta_json),0)) FROM toy_sources s JOIN toys t ON t.id=s.toy_id WHERE t.author_id=?),0)",
    )
    .bind(user_id)
    .bind(user_id)
    .fetch_one(&state.pool)
    .await?;
    Ok(bytes)
}

async fn ensure_storage(
    state: &AppState,
    user_id: &str,
    removed: i64,
    added: i64,
) -> AppResult<()> {
    let used = user_storage(state, user_id).await?;
    if storage_exceeded(used, removed, added, crate::config::MAX_STORAGE_PER_USER) {
        return Err(AppError::status(
            StatusCode::PAYLOAD_TOO_LARGE,
            "storage quota reached",
        ));
    }
    let (total,): (i64,) = sqlx::query_as("SELECT (page_count - freelist_count) * page_size FROM pragma_page_count(), pragma_freelist_count(), pragma_page_size()")
        .fetch_one(&state.pool)
        .await?;
    if total * 10 >= crate::config::MAX_APP_STORAGE * 7 {
        tracing::warn!(used_bytes = total, "application storage is above 70%");
    }
    if storage_exceeded(total, removed, added, crate::config::MAX_APP_STORAGE) {
        return Err(AppError::status(
            StatusCode::INSUFFICIENT_STORAGE,
            "site storage full",
        ));
    }
    Ok(())
}

fn storage_exceeded(used: i64, removed: i64, added: i64, limit: i64) -> bool {
    used.saturating_sub(removed).saturating_add(added) > limit
}

#[cfg(test)]
mod storage_tests {
    use super::storage_exceeded;

    #[test]
    fn replacements_count_only_the_growth() {
        assert!(!storage_exceeded(90, 20, 30, 100));
        assert!(storage_exceeded(90, 10, 30, 100));
    }
}

async fn ensure_toy_capacity(state: &AppState, user_id: &str) -> AppResult<()> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM toys WHERE author_id=?")
        .bind(user_id)
        .fetch_one(&state.pool)
        .await?;
    if count >= crate::config::MAX_TOYS_PER_USER {
        return Err(AppError::status(StatusCode::CONFLICT, "toy quota reached"));
    }
    Ok(())
}

/// Upsert source metadata rows, then push each payload through the blob layer so
/// PPU_BLOB_MODE (db|disk) is honored uniformly. The row is written first (payload
/// column NULL); blobs::store then fills it (db) or writes a file (disk).
async fn write_sources(state: &AppState, toy_id: &str, sources: &[SourceDto]) -> AppResult<()> {
    for s in sources {
        let payload = match &s.payload {
            Some(p) => Some(unb64(p)?),
            None => None,
        };
        if let Some(ref p) = payload {
            if p.len() > crate::config::CAP_SOURCE_PAYLOAD {
                return Err(AppError::status(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "source payload too large",
                ));
            }
        }
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

/// Sweeps the caller's drafts older than `DRAFT_SWEEP_SECS` before the quota check;
/// `forkedFrom` must name a published toy (404 otherwise).
async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<SaveBody>,
) -> AppResult<Response> {
    let tags_json = body
        .tags
        .as_ref()
        .map(|tags| normalize_tags(tags).and_then(|tags| Ok(serde_json::to_string(&tags)?)))
        .transpose()?;
    let save_bytes = validate_save(&body)? + tags_json.as_ref().map_or(2, |tags| tags.len() as i64);
    let _write = state.toy_writes.lock().await;
    // A failed/abandoned upload leaves a draft row behind; sweep the caller's own
    // drafts older than an hour first so they never count against the toy quota.
    // create() only accepts a published forkedFrom, so no draft created from now
    // on is ever a fork target; migration 0008 detached the legacy edges left by
    // the removed fork route, so this delete never trips the forked_from FK.
    sqlx::query("DELETE FROM toys WHERE author_id=? AND state='draft' AND created_at < ?")
        .bind(&user.id)
        .bind(crate::db::now() - crate::config::DRAFT_SWEEP_SECS)
        .execute(&state.pool)
        .await?;
    ensure_toy_capacity(&state, &user.id).await?;
    ensure_storage(&state, &user.id, 0, save_bytes).await?;
    if let Some(src) = &body.forked_from {
        let published: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM toys WHERE id=? AND state='published'")
                .bind(src)
                .fetch_optional(&state.pool)
                .await?;
        if published.is_none() {
            return Err(AppError::status(StatusCode::NOT_FOUND, "no such toy"));
        }
    }
    let id = slug();
    let files_json = serde_json::to_string(&body.files)?;
    let now = crate::db::now();
    sqlx::query("INSERT INTO toys(id,author_id,title,description,files_json,state,forked_from,created_at,tags_json) VALUES(?,?,?,?,?, 'draft', ?, ?, ?)")
        .bind(&id).bind(&user.id).bind(&body.title).bind(&body.description).bind(&files_json).bind(&body.forked_from).bind(now).bind(tags_json.as_deref().unwrap_or("[]")).execute(&state.pool).await?;
    write_sources(&state, &id, &body.sources).await?;
    Ok(Json(serde_json::json!({ "id": id, "revision": 1 })).into_response())
}

async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<SaveBody>,
) -> AppResult<Response> {
    let _write = state.toy_writes.lock().await;
    let author: Option<(String, i64)> =
        sqlx::query_as("SELECT author_id,revision FROM toys WHERE id=?")
            .bind(&id)
            .fetch_optional(&state.pool)
            .await?;
    let (author, current_revision) =
        author.ok_or_else(|| AppError::status(StatusCode::NOT_FOUND, "no such toy"))?;
    if author != user.id {
        return Err(AppError::status(StatusCode::FORBIDDEN, "not your toy"));
    }
    let expected = body.expected_revision.ok_or_else(|| {
        AppError::status(
            StatusCode::PRECONDITION_REQUIRED,
            "missing expectedRevision",
        )
    })?;
    if current_revision != expected {
        return Err(AppError::status(
            StatusCode::CONFLICT,
            "toy changed remotely",
        ));
    }
    // ~1/min throttle applies to every re-save, after the cheap stale-write
    // check so a conflict is never disguised as a rate-limit response.
    if !state.limiter.check_save(&user.id) {
        return Err(AppError::status(
            StatusCode::TOO_MANY_REQUESTS,
            "save rate limit",
        ));
    }
    let tags_json = body
        .tags
        .as_ref()
        .map(|tags| normalize_tags(tags).and_then(|tags| Ok(serde_json::to_string(&tags)?)))
        .transpose()?;
    let (old_bytes, old_tags_bytes): (i64, i64) = sqlx::query_as(
        "SELECT length(title)+length(description)+length(tags_json)+length(files_json)+COALESCE((SELECT SUM(length(name)+length(kind)+COALESCE(length(builtin_id),0)+COALESCE(length(options_json),0)+COALESCE(length(payload),0)+COALESCE(length(meta_json),0)) FROM toy_sources WHERE toy_id=?),0),length(tags_json) FROM toys WHERE id=?",
    )
    .bind(&id)
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;
    let save_bytes = validate_save(&body)?
        + tags_json
            .as_ref()
            .map_or(old_tags_bytes, |tags| tags.len() as i64);
    ensure_storage(&state, &user.id, old_bytes, save_bytes).await?;
    let files_json = serde_json::to_string(&body.files)?;
    let changed = sqlx::query("UPDATE toys SET title=?, description=?, files_json=?, tags_json=COALESCE(?,tags_json), revision=revision+1 WHERE id=? AND revision=?")
        .bind(&body.title)
        .bind(&body.description)
        .bind(&files_json)
        .bind(&tags_json)
        .bind(&id)
        .bind(expected)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if changed == 0 {
        return Err(AppError::status(
            StatusCode::CONFLICT,
            "toy changed remotely",
        ));
    }
    // PUT replaces the whole source set: drop rows that are gone, then upsert the
    // current set (db mode; disk-mode orphan files are the known escape-hatch limit).
    sqlx::query("DELETE FROM toy_sources WHERE toy_id=?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    write_sources(&state, &id, &body.sources).await?;
    Ok(Json(serde_json::json!({ "revision": expected + 1 })).into_response())
}

async fn get_toy(
    State(state): State<AppState>,
    maybe: Option<AuthUser>,
    Path(id): Path<String>,
) -> AppResult<Response> {
    let row: Option<(String,String,String,String,Option<String>,i64,i64,String,String,Option<String>,String)> = sqlx::query_as(
        "SELECT t.title,t.description,t.files_json,t.state,t.forked_from,t.heart_count,t.revision,u.handle,u.id,u.avatar_hash,t.tags_json
         FROM toys t JOIN users u ON u.id=t.author_id WHERE t.id=?").bind(&id).fetch_optional(&state.pool).await?;
    let (
        title,
        description,
        files_json,
        tstate,
        forked_from,
        heart_count,
        revision,
        handle,
        author_id,
        avatar,
        tags_json,
    ) = row.ok_or_else(|| AppError::status(StatusCode::NOT_FOUND, "no such toy"))?;
    if tstate != "published" && maybe.as_ref().is_none_or(|user| user.id != author_id) {
        return Err(AppError::status(StatusCode::NOT_FOUND, "no such toy"));
    }
    let tags: Vec<String> = serde_json::from_str(&tags_json)?;
    let files: serde_json::Value = serde_json::from_str(&files_json)?;
    let src_rows: Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT name,kind,builtin_id,options_json,meta_json FROM toy_sources WHERE toy_id=?",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;
    let mut sources = Vec::new();
    for (name, kind, bid, opts, meta) in src_rows {
        let payload = crate::blobs::load(&state, crate::blobs::BlobKey::Source(&id, &name)).await?;
        sources.push(serde_json::json!({
            "name": name, "kind": kind, "builtinId": bid,
            "options": opts.and_then(|o| serde_json::from_str::<serde_json::Value>(&o).ok()).unwrap_or(serde_json::Value::Null),
            "meta": meta.and_then(|m| serde_json::from_str::<serde_json::Value>(&m).ok()).unwrap_or(serde_json::Value::Null),
            "payload": payload.map(|p| b64(&p)),
        }));
    }
    let hearted = if let Some(u) = &maybe {
        sqlx::query_as::<_, (i64,)>("SELECT 1 FROM hearts WHERE user_id=? AND toy_id=?")
            .bind(&u.id)
            .bind(&id)
            .fetch_optional(&state.pool)
            .await?
            .is_some()
    } else {
        false
    };
    Ok(Json(serde_json::json!({
        "id": id, "title": title, "tags": tags, "description": description, "state": tstate, "revision": revision,
        "files": files, "sources": sources, "heartCount": heart_count, "hearted": hearted, "forkedFrom": forked_from,
        "author": { "id": author_id, "handle": handle, "avatar": avatar },
    })).into_response())
}

/// Publish a draft: validates author + caps, stores clip/thumb blobs, flips
/// state, and (if this is the toy's first publish and Discord is configured)
/// fires the announce webhook in the background — a webhook failure only
/// logs a warning and never fails the publish response itself. Republishing
/// an already-published toy replaces its clip/thumb/title but never
/// re-announces.
async fn publish(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<String>,
    mut mp: Multipart,
) -> AppResult<Response> {
    let _write = state.toy_writes.lock().await;
    let author: Option<(String, Option<i64>)> =
        sqlx::query_as("SELECT author_id, published_at FROM toys WHERE id=?")
            .bind(&id)
            .fetch_optional(&state.pool)
            .await?;
    let (author, published_at) =
        author.ok_or_else(|| AppError::status(StatusCode::NOT_FOUND, "no such toy"))?;
    if author != user.id {
        return Err(AppError::status(StatusCode::FORBIDDEN, "not your toy"));
    }
    let first_publish = published_at.is_none();

    let mut clip: Option<Vec<u8>> = None;
    let mut thumb: Option<Vec<u8>> = None;
    let mut meta_title: Option<String> = None;
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| AppError::status(StatusCode::BAD_REQUEST, format!("bad multipart: {e}")))?
    {
        match field.name().unwrap_or("").to_string().as_str() {
            "meta" => {
                let t = field.text().await.unwrap_or_default();
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
                    meta_title = v["title"].as_str().map(|s| s.to_string());
                }
            }
            "clip" => {
                clip = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| AppError::status(StatusCode::BAD_REQUEST, format!("{e}")))?
                        .to_vec(),
                )
            }
            "thumb" => {
                thumb = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| AppError::status(StatusCode::BAD_REQUEST, format!("{e}")))?
                        .to_vec(),
                )
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }
    let clip = clip.ok_or_else(|| AppError::status(StatusCode::BAD_REQUEST, "missing clip"))?;
    let thumb = thumb.ok_or_else(|| AppError::status(StatusCode::BAD_REQUEST, "missing thumb"))?;
    if meta_title
        .as_ref()
        .is_some_and(|title| title.trim().is_empty() || title.len() > crate::config::CAP_TITLE)
    {
        return Err(AppError::status(StatusCode::BAD_REQUEST, "invalid title"));
    }
    if clip.len() > crate::config::CAP_CLIP {
        return Err(AppError::status(
            StatusCode::PAYLOAD_TOO_LARGE,
            "clip too large",
        ));
    }
    if thumb.len() > crate::config::CAP_THUMB {
        return Err(AppError::status(
            StatusCode::PAYLOAD_TOO_LARGE,
            "thumb too large",
        ));
    }
    let (old_media,): (i64,) = sqlx::query_as(
        "SELECT COALESCE(length(clip),0)+COALESCE(length(thumb),0) FROM toys WHERE id=?",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;
    ensure_storage(
        &state,
        &user.id,
        old_media,
        (clip.len() + thumb.len()) as i64,
    )
    .await?;

    crate::blobs::store(&state, crate::blobs::BlobKey::Clip(&id), &clip).await?;
    crate::blobs::store(&state, crate::blobs::BlobKey::Thumb(&id), &thumb).await?;
    let now = crate::db::now();
    if let Some(t) = &meta_title {
        sqlx::query("UPDATE toys SET title=? WHERE id=?")
            .bind(t)
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
    sqlx::query(
        "UPDATE toys SET state='published', published_at=COALESCE(published_at, ?) WHERE id=?",
    )
    .bind(now)
    .bind(&id)
    .execute(&state.pool)
    .await?;

    if first_publish {
        if let Some(d) = state.cfg.discord.as_ref() {
            if let Some(url) = d.webhook_url.clone() {
                let http = state.http.clone();
                let permalink = format!("{}/t/{}", state.cfg.base_url, id);
                let title = meta_title.clone().unwrap_or_else(|| id.clone());
                tokio::spawn(async move {
                    let form = reqwest::multipart::Form::new()
                        .text("content", format!("New toy: {title}\n{permalink}"))
                        .part(
                            "files[0]",
                            reqwest::multipart::Part::bytes(clip)
                                .file_name("clip.webm")
                                .mime_str("video/webm")
                                .unwrap(),
                        );
                    if let Err(e) = http.post(&url).multipart(form).send().await {
                        tracing::warn!(error=%e, "discord webhook failed");
                    }
                });
            }
        }
    }
    Ok(Json(serde_json::json!({ "id": id, "state": "published" })).into_response())
}

#[derive(Deserialize)]
pub struct WallQuery {
    #[serde(default)]
    sort: Option<String>,
    #[serde(default)]
    page: Option<i64>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    author: Option<String>,
}

const PAGE_SIZE: i64 = 24;

async fn featured(State(state): State<AppState>) -> AppResult<Json<serde_json::Value>> {
    let (id,): (String,) = sqlx::query_as("SELECT value FROM settings WHERE key='featured_toy'")
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(
        serde_json::json!({ "id": if id.is_empty() { None } else { Some(id) } }),
    ))
}

async fn highlights(
    State(state): State<AppState>,
    maybe: Option<AuthUser>,
) -> AppResult<Json<serde_json::Value>> {
    let rows: Vec<(
        String,
        String,
        i64,
        i64,
        String,
        String,
        Option<String>,
        String,
    )> = sqlx::query_as(
        "SELECT t.id,t.title,t.heart_count,t.created_at,u.id,u.handle,u.avatar_hash,t.tags_json
         FROM settings s,json_each(s.value) j
         JOIN toys t ON t.id=j.value JOIN users u ON u.id=t.author_id
         WHERE s.key='featured_toys' AND t.state='published' ORDER BY CAST(j.key AS INTEGER)",
    )
    .fetch_all(&state.pool)
    .await?;
    let uid = maybe.as_ref().map(|user| user.id.as_str());
    let mut cards = Vec::new();
    for (id, title, hearts, created, author_id, handle, avatar, tags_json) in rows {
        let hearted = match uid {
            Some(uid) => {
                sqlx::query_as::<_, (i64,)>("SELECT 1 FROM hearts WHERE user_id=? AND toy_id=?")
                    .bind(uid)
                    .bind(&id)
                    .fetch_optional(&state.pool)
                    .await?
                    .is_some()
            }
            None => false,
        };
        cards.push(wall_card(
            &id, &title, created, &author_id, &handle, &avatar, hearts, hearted, &tags_json,
        ));
    }
    Ok(Json(serde_json::json!({ "toys": cards })))
}

fn wall_card(
    id: &str,
    title: &str,
    created_at: i64,
    author_id: &str,
    handle: &str,
    avatar: &Option<String>,
    heart_count: i64,
    hearted: bool,
    tags_json: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": id, "title": title,
        "author": { "id": author_id, "handle": handle, "avatar": avatar },
        "thumbUrl": format!("/blobs/thumb/{id}"),
        "clipUrl": format!("/blobs/clip/{id}"),
        "heartCount": heart_count, "hearted": hearted,
        "createdAt": created_at,
        "tags": serde_json::from_str::<Vec<String>>(tags_json).unwrap_or_default(),
    })
}

async fn wall(
    State(state): State<AppState>,
    maybe: Option<AuthUser>,
    Query(q): Query<WallQuery>,
) -> AppResult<Response> {
    let page = q.page.unwrap_or(0).max(0);
    let query = q.q.unwrap_or_default();
    let tag = q.tag.unwrap_or_default().trim().to_ascii_lowercase();
    let author = q.author.unwrap_or_default();
    let order = match q.sort.as_deref() {
        Some("popular") => "t.heart_count DESC, t.created_at DESC, t.id DESC",
        _ => "t.created_at DESC, t.id DESC",
    };
    // ponytail: scan at most five JSON tags per toy; use an indexed tag table if filtering becomes a bottleneck.
    let sql = format!("SELECT t.id,t.title,t.heart_count,t.created_at,u.id,u.handle,u.avatar_hash,t.tags_json FROM toys t JOIN users u ON u.id=t.author_id
                       WHERE t.state='published' AND (?='' OR t.title LIKE '%'||?||'%' OR u.handle LIKE '%'||?||'%') AND (?='' OR EXISTS (SELECT 1 FROM json_each(t.tags_json) WHERE value=?)) AND (?='' OR u.handle=?) ORDER BY {order} LIMIT ? OFFSET ?");
    let rows: Vec<(
        String,
        String,
        i64,
        i64,
        String,
        String,
        Option<String>,
        String,
    )> = sqlx::query_as(&sql)
        .bind(&query)
        .bind(&query)
        .bind(&query)
        .bind(&tag)
        .bind(&tag)
        .bind(&author)
        .bind(&author)
        .bind(PAGE_SIZE + 1)
        .bind(page * PAGE_SIZE)
        .fetch_all(&state.pool)
        .await?;
    let uid = maybe.as_ref().map(|u| u.id.clone());
    let has_more = rows.len() as i64 > PAGE_SIZE;
    let mut cards = Vec::new();
    for (id, title, hc, created, author_id, handle, avatar, tags_json) in
        rows.into_iter().take(PAGE_SIZE as usize)
    {
        let hearted = match &uid {
            Some(u) => {
                sqlx::query_as::<_, (i64,)>("SELECT 1 FROM hearts WHERE user_id=? AND toy_id=?")
                    .bind(u)
                    .bind(&id)
                    .fetch_optional(&state.pool)
                    .await?
                    .is_some()
            }
            None => false,
        };
        cards.push(wall_card(
            &id, &title, created, &author_id, &handle, &avatar, hc, hearted, &tags_json,
        ));
    }
    Ok(Json(serde_json::json!({ "toys": cards, "nextPage": if has_more { Some(page+1) } else { None } })).into_response())
}

async fn profile(
    State(state): State<AppState>,
    maybe: Option<AuthUser>,
    Path(handle): Path<String>,
) -> AppResult<Response> {
    let u: Option<(String, String, Option<String>)> =
        sqlx::query_as("SELECT id,handle,avatar_hash FROM users WHERE handle=?")
            .bind(&handle)
            .fetch_optional(&state.pool)
            .await?;
    let (uid, handle, avatar) =
        u.ok_or_else(|| AppError::status(StatusCode::NOT_FOUND, "no such user"))?;
    let rows: Vec<(String,String,i64,i64,String)> = sqlx::query_as("SELECT id,title,heart_count,created_at,tags_json FROM toys WHERE author_id=? AND state='published' ORDER BY created_at DESC").bind(&uid).fetch_all(&state.pool).await?;
    let viewer = maybe.as_ref().map(|x| x.id.clone());
    let mut cards = Vec::new();
    for (id, title, hc, created, tags_json) in rows {
        let hearted = match &viewer {
            Some(v) => {
                sqlx::query_as::<_, (i64,)>("SELECT 1 FROM hearts WHERE user_id=? AND toy_id=?")
                    .bind(v)
                    .bind(&id)
                    .fetch_optional(&state.pool)
                    .await?
                    .is_some()
            }
            None => false,
        };
        cards.push(wall_card(
            &id, &title, created, &uid, &handle, &avatar, hc, hearted, &tags_json,
        ));
    }
    Ok(Json(serde_json::json!({ "user": { "id": uid, "handle": handle, "avatar": avatar }, "toys": cards })).into_response())
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/toys", get(wall).post(create))
        .route("/toys/{id}", get(get_toy).put(update))
        .route("/toys/{id}/publish", post(publish))
        .route("/featured", get(featured))
        .route("/highlights", get(highlights))
        .route("/users/{handle}", get(profile))
}
