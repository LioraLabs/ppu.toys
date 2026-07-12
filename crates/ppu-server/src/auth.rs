use axum::extract::{FromRequestParts, State};
use axum::http::{request::Parts, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use crate::error::AppError;
use crate::error::AppResult;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "ppu_sess";
pub const STATE_COOKIE: &str = "ppu_oauth_state";
pub const CSRF_HEADER: &str = "x-ppu-csrf";

pub struct AuthUser { pub id: String, pub handle: String, pub is_admin: bool }

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        if matches!(parts.method, Method::POST | Method::PUT | Method::DELETE)
            && parts.headers.get(CSRF_HEADER).is_none() {
            return Err(AppError::status(StatusCode::FORBIDDEN, "missing X-PPU-CSRF header"));
        }
        let jar = CookieJar::from_headers(&parts.headers);
        let sid = jar.get(SESSION_COOKIE).map(|c| c.value().to_string())
            .ok_or_else(|| AppError::status(StatusCode::UNAUTHORIZED, "no session"))?;
        let now = crate::db::now();
        let row: Option<(String, String, i64)> = sqlx::query_as(
            "SELECT u.id, u.handle, u.is_admin FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.id=? AND s.expires_at > ?"
        ).bind(&sid).bind(now).fetch_optional(&state.pool).await?;
        let (id, handle, is_admin) = row.ok_or_else(|| AppError::status(StatusCode::UNAUTHORIZED, "invalid session"))?;
        Ok(AuthUser { id, handle, is_admin: is_admin != 0 })
    }
}

impl FromRequestParts<AppState> for Option<AuthUser> {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        Ok(AuthUser::from_request_parts(parts, state).await.ok())
    }
}

pub fn build_cookie(sid: String, ttl_days: i64) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, sid))
        .http_only(true).secure(true).same_site(SameSite::Lax).path("/")
        .max_age(time::Duration::days(ttl_days))
        .build()
}

async fn me(user: AuthUser) -> impl IntoResponse {
    Json(serde_json::json!({ "id": user.id, "handle": user.handle, "isAdmin": user.is_admin }))
}

async fn logout(State(state): State<AppState>, _user: AuthUser, jar: CookieJar) -> AppResult<Response> {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        sqlx::query("DELETE FROM sessions WHERE id=?").bind(c.value()).execute(&state.pool).await?;
    }
    let jar = jar.remove(Cookie::from(SESSION_COOKIE));
    Ok((jar, StatusCode::NO_CONTENT).into_response())
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/me", get(me)).route("/auth/logout", post(logout))
}
