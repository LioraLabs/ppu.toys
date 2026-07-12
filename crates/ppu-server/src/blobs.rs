use crate::error::AppResult;
use crate::config::BlobMode;
use crate::state::AppState;

#[derive(Clone, Copy)]
pub enum BlobKey<'a> { Clip(&'a str), Thumb(&'a str), Source(&'a str, &'a str) }

impl<'a> BlobKey<'a> {
    fn disk_path(&self, root: &std::path::Path) -> std::path::PathBuf {
        match self {
            BlobKey::Clip(id) => root.join("clip").join(id),
            BlobKey::Thumb(id) => root.join("thumb").join(id),
            BlobKey::Source(t, n) => root.join("src").join(format!("{t}__{n}")),
        }
    }
}

pub async fn store(state: &AppState, key: BlobKey<'_>, bytes: &[u8]) -> AppResult<()> {
    match state.cfg.blob_mode {
        BlobMode::Db => match key {
            BlobKey::Clip(id) => { sqlx::query("UPDATE toys SET clip=? WHERE id=?").bind(bytes).bind(id).execute(&state.pool).await?; }
            BlobKey::Thumb(id) => { sqlx::query("UPDATE toys SET thumb=? WHERE id=?").bind(bytes).bind(id).execute(&state.pool).await?; }
            BlobKey::Source(t, n) => { sqlx::query("UPDATE toy_sources SET payload=? WHERE toy_id=? AND name=?").bind(bytes).bind(t).bind(n).execute(&state.pool).await?; }
        },
        BlobMode::Disk => {
            let p = key.disk_path(&state.cfg.data_dir);
            if let Some(parent) = p.parent() { tokio::fs::create_dir_all(parent).await?; }
            tokio::fs::write(&p, bytes).await?;
        }
    }
    Ok(())
}

pub async fn load(state: &AppState, key: BlobKey<'_>) -> AppResult<Option<Vec<u8>>> {
    match state.cfg.blob_mode {
        BlobMode::Db => {
            let row: Option<(Option<Vec<u8>>,)> = match key {
                BlobKey::Clip(id) => sqlx::query_as("SELECT clip FROM toys WHERE id=?").bind(id).fetch_optional(&state.pool).await?,
                BlobKey::Thumb(id) => sqlx::query_as("SELECT thumb FROM toys WHERE id=?").bind(id).fetch_optional(&state.pool).await?,
                BlobKey::Source(t, n) => sqlx::query_as("SELECT payload FROM toy_sources WHERE toy_id=? AND name=?").bind(t).bind(n).fetch_optional(&state.pool).await?,
            };
            Ok(row.and_then(|(b,)| b))
        }
        BlobMode::Disk => {
            let p = key.disk_path(&state.cfg.data_dir);
            match tokio::fs::read(&p).await { Ok(b) => Ok(Some(b)), Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None), Err(e) => Err(e.into()) }
        }
    }
}

// serving handlers replace this stub in Task 9
pub fn routes() -> axum::Router<AppState> { axum::Router::new() }
