mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use ppu_server::config::{BlobMode, DiscordConfig};
use tower::ServiceExt;

async fn mock_discord() -> String {
    let app = Router::new()
        .route("/token", post(|| async { Json(serde_json::json!({ "access_token": "at", "token_type": "Bearer" })) }))
        .route("/users/@me", get(|| async { Json(serde_json::json!({ "id": "9001", "username": "trinity", "avatar": "abc" })) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
    base
}

fn discord_cfg(base: &str) -> DiscordConfig {
    DiscordConfig {
        client_id: "cid".into(), client_secret: "sec".into(),
        redirect_uri: "http://test.local/api/auth/callback".into(),
        authorize_url: "https://discord.com/oauth2/authorize".into(),
        token_url: format!("{base}/token"),
        userinfo_url: format!("{base}/users/@me"),
        webhook_url: None,
    }
}

#[tokio::test]
async fn auth_disabled_returns_503() {
    let app = common::test_app().await;
    let res = app.router.clone().oneshot(Request::builder().uri("/api/auth/discord").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn discord_redirects_to_authorize_with_state_cookie() {
    let base = mock_discord().await;
    let app = common::test_app_with(Some(discord_cfg(&base)), BlobMode::Db).await;
    let res = app.router.clone().oneshot(Request::builder().uri("/api/auth/discord").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let loc = res.headers().get("location").unwrap().to_str().unwrap();
    assert!(loc.starts_with("https://discord.com/oauth2/authorize"));
    assert!(loc.contains("scope=identify"));
    assert!(res.headers().get("set-cookie").unwrap().to_str().unwrap().contains("ppu_oauth_state="));
}

#[tokio::test]
async fn callback_upserts_user_mints_session_sets_cookie() {
    let base = mock_discord().await;
    let app = common::test_app_with(Some(discord_cfg(&base)), BlobMode::Db).await;
    let res = app.router.clone().oneshot(
        Request::builder().uri("/api/auth/callback?code=xyz&state=st")
            .header("cookie", "ppu_oauth_state=st").body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let set = res.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(set.contains("ppu_sess="));
    let (h,): (String,) = sqlx::query_as("SELECT handle FROM users WHERE id='9001'").fetch_one(&app.state.pool).await.unwrap();
    assert_eq!(h, "trinity");
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sessions").fetch_one(&app.state.pool).await.unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn callback_rejects_state_mismatch() {
    let base = mock_discord().await;
    let app = common::test_app_with(Some(discord_cfg(&base)), BlobMode::Db).await;
    let res = app.router.clone().oneshot(
        Request::builder().uri("/api/auth/callback?code=xyz&state=st")
            .header("cookie", "ppu_oauth_state=DIFFERENT").body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn callback_rejects_banned_discord_id() {
    let base = mock_discord().await;
    let app = common::test_app_with(Some(discord_cfg(&base)), BlobMode::Db).await;
    // mock userinfo returns id "9001"; pre-ban it
    sqlx::query("INSERT INTO bans(discord_id,created_at) VALUES('9001',1)").execute(&app.state.pool).await.unwrap();
    let res = app.router.clone().oneshot(
        Request::builder().uri("/api/auth/callback?code=xyz&state=st")
            .header("cookie", "ppu_oauth_state=st").body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sessions").fetch_one(&app.state.pool).await.unwrap();
    assert_eq!(n, 0, "no session minted for a banned id");
}
