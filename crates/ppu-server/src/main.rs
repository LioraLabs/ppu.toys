use ppu_server::{config::Config, db, state::AppState};

/// `ppu-server mint-session <handle>` — ensure a SYSTEM user (non-Discord id
/// `sys:<handle>`) and print a fresh session id for it, then exit.
/// Runbook use: seeding the official demo account on the box —
///   sudo -u ppu /opt/ppu/ppu-server mint-session ppu
/// The printed value goes into the seeding script's PPU_SEED_COOKIE.
async fn mint_session(handle: &str) -> anyhow::Result<()> {
    let cfg = Config::from_env();
    let pool = db::connect(&cfg.db_path).await?;
    db::migrate(&pool).await?;
    let now = db::now();
    let uid = format!("sys:{handle}");
    sqlx::query(
        "INSERT INTO users(id,handle,is_admin,created_at) VALUES(?,?,0,?)
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(&uid)
    .bind(handle)
    .bind(now)
    .execute(&pool)
    .await?;
    let sid = ppu_server::auth::rand_hex(16);
    sqlx::query("INSERT INTO sessions(id,user_id,created_at,expires_at) VALUES(?,?,?,?)")
        .bind(&sid)
        .bind(&uid)
        .bind(now)
        .bind(now + 7 * 86400) // one week: seeding is a sit-down task, not a standing credential
        .execute(&pool)
        .await?;
    println!("{sid}");
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let [_, cmd, handle] = &args[..] {
        if cmd == "mint-session" {
            return mint_session(handle).await;
        }
    }
    if args.len() > 1 {
        anyhow::bail!("unknown arguments (supported: mint-session <handle>)");
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ppu_server=info,tower_http=info".into()),
        )
        .init();
    let cfg = Config::from_env();
    let pool = db::connect(&cfg.db_path).await?;
    db::migrate(&pool).await?;
    let addr = format!("127.0.0.1:{}", cfg.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, discord = cfg.discord.is_some(), "ppu-server listening");
    let state = AppState::new(cfg, pool);
    ppu_server::serve(state, listener).await
}
