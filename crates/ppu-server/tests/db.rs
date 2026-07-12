use ppu_server::db;

#[tokio::test]
async fn migrate_creates_schema_and_heart_trigger_counts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    let pool = db::connect(path.to_str().unwrap()).await.unwrap();
    db::migrate(&pool).await.unwrap();

    let now = 1_700_000_000i64;
    sqlx::query("INSERT INTO users(id,handle,created_at) VALUES('u1','ann',?)").bind(now).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,created_at) VALUES('toy1','u1','t','[]',?)").bind(now).execute(&pool).await.unwrap();

    for _ in 0..2 {
        sqlx::query("INSERT OR IGNORE INTO hearts(user_id,toy_id,created_at) VALUES('u1','toy1',?)").bind(now).execute(&pool).await.unwrap();
    }
    let (c,): (i64,) = sqlx::query_as("SELECT heart_count FROM toys WHERE id='toy1'").fetch_one(&pool).await.unwrap();
    assert_eq!(c, 1);

    sqlx::query("DELETE FROM hearts WHERE user_id='u1' AND toy_id='toy1'").execute(&pool).await.unwrap();
    let (c,): (i64,) = sqlx::query_as("SELECT heart_count FROM toys WHERE id='toy1'").fetch_one(&pool).await.unwrap();
    assert_eq!(c, 0);
}
