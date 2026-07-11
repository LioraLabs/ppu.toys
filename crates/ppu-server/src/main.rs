use ppu_server::{config::Config, db, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "ppu_server=info,tower_http=info".into())
    ).init();
    let cfg = Config::from_env();
    let pool = db::connect(&cfg.db_path).await?;
    db::migrate(&pool).await?;
    let addr = format!("127.0.0.1:{}", cfg.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, discord = cfg.discord.is_some(), "ppu-server listening");
    let state = AppState::new(cfg, pool);
    ppu_server::serve(state, listener).await
}
