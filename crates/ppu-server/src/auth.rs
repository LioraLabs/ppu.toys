use crate::error::AppError;
use crate::error::AppResult;
use crate::state::AppState;
use axum::extract::{FromRequestParts, Query, State};
use axum::http::{request::Parts, Method, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use rand::Rng;
use sha2::{Digest, Sha256};

pub const SESSION_COOKIE: &str = "ppu_sess";
pub const STATE_COOKIE: &str = "ppu_oauth_state";
pub const CSRF_HEADER: &str = "x-ppu-csrf";

pub struct AuthUser {
    pub id: String,
    pub handle: String,
    pub avatar: Option<String>,
    pub is_admin: bool,
    pub api_token: bool,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let bearer = parts
            .headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "));
        if let Some(token) = bearer {
            let hash = token_hash(token);
            let row: Option<(String, String, Option<String>, i64)> = sqlx::query_as(
                "SELECT u.id,u.handle,u.avatar_hash,u.is_admin FROM api_tokens t JOIN users u ON u.id=t.user_id WHERE t.token_hash=?",
            )
            .bind(hash)
            .fetch_optional(&state.pool)
            .await?;
            let (id, handle, avatar, is_admin) =
                row.ok_or_else(|| AppError::status(StatusCode::UNAUTHORIZED, "invalid token"))?;
            return Ok(AuthUser {
                id,
                handle,
                avatar,
                is_admin: is_admin != 0,
                api_token: true,
            });
        }
        if matches!(parts.method, Method::POST | Method::PUT | Method::DELETE)
            && parts.headers.get(CSRF_HEADER).is_none()
        {
            return Err(AppError::status(
                StatusCode::FORBIDDEN,
                "missing X-PPU-CSRF header",
            ));
        }
        let jar = CookieJar::from_headers(&parts.headers);
        let sid = jar
            .get(SESSION_COOKIE)
            .map(|c| c.value().to_string())
            .ok_or_else(|| AppError::status(StatusCode::UNAUTHORIZED, "no session"))?;
        let now = crate::db::now();
        let row: Option<(String, String, Option<String>, i64)> = sqlx::query_as(
            "SELECT u.id, u.handle, u.avatar_hash, u.is_admin FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.id=? AND s.expires_at > ?"
        ).bind(&sid).bind(now).fetch_optional(&state.pool).await?;
        let (id, handle, avatar, is_admin) =
            row.ok_or_else(|| AppError::status(StatusCode::UNAUTHORIZED, "invalid session"))?;
        Ok(AuthUser {
            id,
            handle,
            avatar,
            is_admin: is_admin != 0,
            api_token: false,
        })
    }
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

impl FromRequestParts<AppState> for Option<AuthUser> {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(AuthUser::from_request_parts(parts, state).await.ok())
    }
}

pub fn build_cookie(sid: String, ttl_days: i64) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, sid))
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::days(ttl_days))
        .build()
}

/// A cookie shaped for removal. `CookieJar::remove` only emits a clearing
/// Set-Cookie for the SAME path the cookie was set with (RFC 6265); our cookies
/// use `Path=/`, so removals must too or the browser keeps the cookie.
fn removal_cookie(name: &'static str) -> Cookie<'static> {
    Cookie::build((name, "")).path("/").build()
}

async fn me(user: AuthUser) -> impl IntoResponse {
    Json(
        serde_json::json!({ "id": user.id, "handle": user.handle, "avatar": user.avatar, "isAdmin": user.is_admin }),
    )
}

async fn logout(
    State(state): State<AppState>,
    _user: AuthUser,
    jar: CookieJar,
) -> AppResult<Response> {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        sqlx::query("DELETE FROM sessions WHERE id=?")
            .bind(c.value())
            .execute(&state.pool)
            .await?;
    }
    let jar = jar.remove(removal_cookie(SESSION_COOKIE));
    Ok((jar, StatusCode::NO_CONTENT).into_response())
}

#[derive(serde::Deserialize)]
struct NewToken {
    #[serde(default = "default_token_name")]
    name: String,
}

fn default_token_name() -> String {
    "CLI".into()
}

async fn list_tokens(State(state): State<AppState>, user: AuthUser) -> AppResult<Response> {
    if user.api_token {
        return Err(AppError::status(
            StatusCode::FORBIDDEN,
            "manage tokens from the website",
        ));
    }
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT id,name,created_at FROM api_tokens WHERE user_id=? ORDER BY created_at DESC",
    )
    .bind(&user.id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, name, created_at)| {
                serde_json::json!({
                    "id": id, "name": name, "createdAt": created_at
                })
            })
            .collect::<Vec<_>>(),
    )
    .into_response())
}

async fn create_token(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<NewToken>,
) -> AppResult<Response> {
    if user.api_token {
        return Err(AppError::status(
            StatusCode::FORBIDDEN,
            "manage tokens from the website",
        ));
    }
    let name = body.name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(AppError::status(
            StatusCode::BAD_REQUEST,
            "token name must be 1-80 characters",
        ));
    }
    let id = rand_hex(8);
    let token = format!("ppu_{}", rand_hex(32));
    sqlx::query("INSERT INTO api_tokens(id,user_id,name,token_hash,created_at) VALUES(?,?,?,?,?)")
        .bind(&id)
        .bind(&user.id)
        .bind(name)
        .bind(token_hash(&token))
        .bind(crate::db::now())
        .execute(&state.pool)
        .await?;
    Ok(Json(serde_json::json!({ "id": id, "name": name, "token": token })).into_response())
}

async fn delete_token(
    State(state): State<AppState>,
    user: AuthUser,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> AppResult<Response> {
    if user.api_token {
        return Err(AppError::status(
            StatusCode::FORBIDDEN,
            "manage tokens from the website",
        ));
    }
    sqlx::query("DELETE FROM api_tokens WHERE id=? AND user_id=?")
        .bind(id)
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

pub fn rand_hex(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    rand::thread_rng().fill(&mut b[..]);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

async fn discord_start(State(state): State<AppState>, jar: CookieJar) -> AppResult<Response> {
    let d = state.cfg.discord.as_ref().ok_or_else(|| {
        AppError::status(
            StatusCode::SERVICE_UNAVAILABLE,
            "Discord sign-in is not configured",
        )
    })?;
    let csrf = rand_hex(16);
    let url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope=identify&state={}",
        d.authorize_url,
        urlencode(&d.client_id),
        urlencode(&d.redirect_uri),
        csrf
    );
    let state_cookie = Cookie::build((STATE_COOKIE, csrf))
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .path("/")
        .build();
    let jar = jar.add(state_cookie);
    Ok((jar, Redirect::to(&url)).into_response())
}

#[derive(serde::Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}
#[derive(serde::Deserialize)]
struct TokenResp {
    access_token: String,
}
#[derive(serde::Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
    avatar: Option<String>,
}

async fn discord_callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<CallbackQuery>,
) -> AppResult<Response> {
    let d = state.cfg.discord.as_ref().ok_or_else(|| {
        AppError::status(
            StatusCode::SERVICE_UNAVAILABLE,
            "Discord sign-in is not configured",
        )
    })?;
    let expect = jar.get(STATE_COOKIE).map(|c| c.value().to_string());
    if expect.as_deref() != Some(q.state.as_str()) {
        return Err(AppError::status(StatusCode::BAD_REQUEST, "state mismatch"));
    }
    let token: TokenResp = state
        .http
        .post(&d.token_url)
        .form(&[
            ("client_id", d.client_id.as_str()),
            ("client_secret", d.client_secret.as_str()),
            ("grant_type", "authorization_code"),
            ("code", q.code.as_str()),
            ("redirect_uri", d.redirect_uri.as_str()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let du: DiscordUser = state
        .http
        .get(&d.userinfo_url)
        .bearer_auth(&token.access_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let banned: Option<(String,)> =
        sqlx::query_as("SELECT discord_id FROM bans WHERE discord_id=?")
            .bind(&du.id)
            .fetch_optional(&state.pool)
            .await?;
    if banned.is_some() {
        return Err(AppError::status(StatusCode::FORBIDDEN, "account banned"));
    }

    let is_admin = state.cfg.admin_ids.iter().any(|a| a == &du.id) as i64;
    let now = crate::db::now();
    sqlx::query("INSERT INTO users(id,handle,avatar_hash,is_admin,created_at) VALUES(?,?,?,?,?)
                 ON CONFLICT(id) DO UPDATE SET handle=excluded.handle, avatar_hash=excluded.avatar_hash, is_admin=excluded.is_admin")
        .bind(&du.id).bind(&du.username).bind(&du.avatar).bind(is_admin).bind(now)
        .execute(&state.pool).await?;
    let sid = rand_hex(16);
    sqlx::query("INSERT INTO sessions(id,user_id,created_at,expires_at) VALUES(?,?,?,?)")
        .bind(&sid)
        .bind(&du.id)
        .bind(now)
        .bind(now + state.cfg.session_ttl_days * 86400)
        .execute(&state.pool)
        .await?;
    // Two single-cookie jars applied in order, rather than one jar with both an add and a
    // remove: `cookie::CookieJar` stores its delta in a `HashSet`, so combining both mutations
    // into one jar makes the emitted Set-Cookie header order (and thus which cookie
    // `HeaderMap::get("set-cookie")` returns) nondeterministic across requests.
    let session_jar = CookieJar::new().add(build_cookie(sid, state.cfg.session_ttl_days));
    let removal_jar = jar.remove(removal_cookie(STATE_COOKIE));
    Ok((session_jar, removal_jar, Redirect::to("/")).into_response())
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/me", get(me))
        .route("/tokens", get(list_tokens).post(create_token))
        .route("/tokens/{id}", axum::routing::delete(delete_token))
        .route("/auth/logout", post(logout))
        .route("/auth/discord", get(discord_start))
        .route("/auth/callback", get(discord_callback))
}
