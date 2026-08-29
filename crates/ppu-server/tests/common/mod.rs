#![allow(dead_code)]
use axum::Router;
use ppu_server::config::{BlobMode, Config, DiscordConfig};
use ppu_server::db;
use ppu_server::state::AppState;
use std::path::Path;
use tempfile::TempDir;

pub struct TestApp {
    pub router: Router,
    pub state: AppState,
    pub dir: TempDir,
}

pub async fn test_app() -> TestApp {
    build(None, BlobMode::Db, None, None).await
}
pub async fn test_app_with(discord: Option<DiscordConfig>, blob_mode: BlobMode) -> TestApp {
    build(discord, blob_mode, None, None).await
}
pub async fn test_app_dev() -> TestApp {
    build(None, BlobMode::Db, None, Some("local-only".into())).await
}
pub async fn test_app_web(web_dir: &Path) -> TestApp {
    build(None, BlobMode::Db, Some(web_dir.to_path_buf()), None).await
}

async fn build(
    discord: Option<DiscordConfig>,
    blob_mode: BlobMode,
    web_dir: Option<std::path::PathBuf>,
    dev_token: Option<String>,
) -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::from_map(|_| None);
    cfg.db_path = dir.path().join("t.db").to_str().unwrap().to_string();
    cfg.data_dir = dir.path().join("data");
    cfg.blob_mode = blob_mode;
    cfg.discord = discord;
    cfg.base_url = "http://test.local".into();
    cfg.dev_token = dev_token;
    if let Some(w) = web_dir {
        cfg.web_dir = w;
    }
    let pool = db::connect(&cfg.db_path).await.unwrap();
    db::migrate(&pool).await.unwrap();
    if cfg.dev_token.is_some() {
        sqlx::query("INSERT INTO users(id,handle,created_at) VALUES('sys:ppu','ppu',0)")
            .execute(&pool)
            .await
            .unwrap();
    }
    let state = AppState::new(cfg, pool);
    let router = ppu_server::build_router(state.clone());
    TestApp { router, state, dir }
}

/// Insert a user (optionally admin) and a live session; returns session id.
pub async fn seed_session(state: &AppState, user_id: &str, handle: &str, admin: bool) -> String {
    let now = ppu_server::db::now();
    sqlx::query("INSERT INTO users(id,handle,is_admin,created_at) VALUES(?,?,?,?)")
        .bind(user_id)
        .bind(handle)
        .bind(admin as i64)
        .bind(now)
        .execute(&state.pool)
        .await
        .unwrap();
    let sid = format!("sess_{user_id}");
    sqlx::query("INSERT INTO sessions(id,user_id,created_at,expires_at) VALUES(?,?,?,?)")
        .bind(&sid)
        .bind(user_id)
        .bind(now)
        .bind(now + 86400)
        .execute(&state.pool)
        .await
        .unwrap();
    sid
}
