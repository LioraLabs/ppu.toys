mod common;
use ppu_server::blobs::{self, BlobKey};
use ppu_server::config::BlobMode;

async fn roundtrip(mode: BlobMode) {
    let app = common::test_app_with(None, mode).await;
    let now = ppu_server::db::now();
    sqlx::query("INSERT INTO users(id,handle,created_at) VALUES('u','h',?)").bind(now).execute(&app.state.pool).await.unwrap();
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,created_at) VALUES('toyA','u','t','[]',?)").bind(now).execute(&app.state.pool).await.unwrap();

    blobs::store(&app.state, BlobKey::Clip("toyA"), b"clipbytes").await.unwrap();
    let got = blobs::load(&app.state, BlobKey::Clip("toyA")).await.unwrap();
    assert_eq!(got.as_deref(), Some(&b"clipbytes"[..]));

    let missing = blobs::load(&app.state, BlobKey::Thumb("toyA")).await.unwrap();
    assert_eq!(missing, None);

    // Source payload roundtrip. In db mode the row must pre-exist (payload is a
    // column on it); a name with path-hazard chars must be handled safely.
    sqlx::query("INSERT INTO toy_sources(toy_id,name,kind) VALUES('toyA','a/../b','bg')").execute(&app.state.pool).await.unwrap();
    blobs::store(&app.state, BlobKey::Source("toyA", "a/../b"), b"srcbytes").await.unwrap();
    let got = blobs::load(&app.state, BlobKey::Source("toyA", "a/../b")).await.unwrap();
    assert_eq!(got.as_deref(), Some(&b"srcbytes"[..]));
}

#[tokio::test]
async fn blob_roundtrip_db_mode() { roundtrip(BlobMode::Db).await; }
#[tokio::test]
async fn blob_roundtrip_disk_mode() { roundtrip(BlobMode::Disk).await; }
