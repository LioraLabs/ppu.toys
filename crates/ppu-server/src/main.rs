use ppu_server::{config::Config, db, state::AppState};
use sha2::{Digest, Sha256};

async fn ensure_dev_account(pool: &sqlx::SqlitePool, token: &str) -> anyhow::Result<()> {
    let now = db::now();
    let hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    sqlx::query("INSERT INTO users(id,handle,created_at) VALUES('sys:ppu','ppu',?) ON CONFLICT(id) DO UPDATE SET handle='ppu'")
        .bind(now)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO api_tokens(id,user_id,name,token_hash,created_at) VALUES('local-demo-cli','sys:ppu','Local demos',?,?) ON CONFLICT(id) DO UPDATE SET token_hash=excluded.token_hash")
        .bind(hash)
        .bind(now)
        .execute(pool)
        .await?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ppu_server=info,tower_http=info".into()),
        )
        .init();
    let cfg = Config::from_env();
    let pool = db::connect(&cfg.db_path).await?;
    db::migrate(&pool).await?;
    if let Some(token) = &cfg.dev_token {
        ensure_dev_account(&pool, token).await?;
    }
    let addr = format!("127.0.0.1:{}", cfg.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, discord = cfg.discord.is_some(), "ppu-server listening");
    let state = AppState::new(cfg, pool);
    ppu_server::serve(state, listener).await
}
