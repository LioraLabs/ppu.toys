use ppu_server::{config::Config, db, state::AppState};

async fn ensure_dev_account(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    let now = db::now();
    sqlx::query("INSERT INTO users(id,handle,is_admin,created_at) VALUES('sys:ppu','ppu',1,?) ON CONFLICT(id) DO UPDATE SET handle='ppu', is_admin=1")
        .bind(now)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO toys(id,author_id,title,description,files_json,state,created_at,published_at) SELECT 'local-featured','sys:ppu','Local Signal','Bundled local demo',json_extract(value,'$.files'),'published',?,? FROM settings WHERE key='starter_template' AND NOT EXISTS(SELECT 1 FROM toys WHERE state='published')")
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;
    sqlx::query("UPDATE settings SET value=(SELECT id FROM toys WHERE state='published' ORDER BY title!='mode7-road',created_at LIMIT 1) WHERE key='featured_toy' AND value=''")
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
    if cfg.dev_seed {
        ensure_dev_account(&pool).await?;
    }
    let addr = format!("127.0.0.1:{}", cfg.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, discord = cfg.discord.is_some(), "ppu-server listening");
    let state = AppState::new(cfg, pool);
    ppu_server::serve(state, listener).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dev_account_seeds_the_empty_stack_with_a_featured_toy() {
        let pool = db::connect(":memory:").await.unwrap();
        db::migrate(&pool).await.unwrap();
        ensure_dev_account(&pool).await.unwrap();
        let (featured,): (String,) =
            sqlx::query_as("SELECT value FROM settings WHERE key='featured_toy'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(featured, "local-featured");

        sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('road','sys:ppu','mode7-road','[]','published',1)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE settings SET value='' WHERE key='featured_toy'")
            .execute(&pool)
            .await
            .unwrap();
        ensure_dev_account(&pool).await.unwrap();
        let (featured,): (String,) =
            sqlx::query_as("SELECT value FROM settings WHERE key='featured_toy'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(featured, "road");
    }
}
