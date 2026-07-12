mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn fork_copies_files_and_sources_for_caller() {
    let app = common::test_app().await;
    let owner = common::seed_session(&app.state, "1", "ann", false).await;
    let forker = common::seed_session(&app.state, "2", "bob", false).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('orig','1','O','[{\"name\":\"m.lua\",\"source\":\"x\"}]','published',1)").execute(&app.state.pool).await.unwrap();
    sqlx::query("INSERT INTO toy_sources(toy_id,name,kind,payload) VALUES('orig','bg1','bg',?)").bind(&b"data"[..]).execute(&app.state.pool).await.unwrap();

    let res = app.router.clone().oneshot(Request::builder().method("POST").uri("/api/toys/orig/fork")
        .header("cookie", format!("ppu_sess={forker}")).header("x-ppu-csrf","1").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let b = axum::body::to_bytes(res.into_body(), 1<<20).await.unwrap();
    let nid = serde_json::from_slice::<serde_json::Value>(&b).unwrap()["id"].as_str().unwrap().to_string();

    let (author, forked, state): (String,Option<String>,String) = sqlx::query_as("SELECT author_id,forked_from,state FROM toys WHERE id=?").bind(&nid).fetch_one(&app.state.pool).await.unwrap();
    assert_eq!(author, "2");
    assert_eq!(forked.as_deref(), Some("orig"));
    assert_eq!(state, "draft");
    let (n, payload): (i64, Option<Vec<u8>>) = sqlx::query_as("SELECT COUNT(*), MAX(payload) FROM toy_sources WHERE toy_id=?").bind(&nid).fetch_one(&app.state.pool).await.unwrap();
    assert_eq!(n, 1);
    assert_eq!(payload.as_deref(), Some(&b"data"[..]), "payload copied");
    let _ = owner;
}

#[tokio::test]
async fn fork_missing_toy_404() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "2", "bob", false).await;
    let res = app.router.clone().oneshot(Request::builder().method("POST").uri("/api/toys/nope/fork")
        .header("cookie", format!("ppu_sess={sid}")).header("x-ppu-csrf","1").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
