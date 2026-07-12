mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

fn authed(method: &str, uri: &str, sid: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder().method(method).uri(uri)
        .header("cookie", format!("ppu_sess={sid}"))
        .header("x-ppu-csrf", "1").header("content-type", "application/json")
        .body(Body::from(body.to_string())).unwrap()
}

#[tokio::test]
async fn create_get_update_snapshots_revision() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;

    let res = app.router.clone().oneshot(authed("POST", "/api/toys", &sid, serde_json::json!({
        "title": "Hi", "files": [{"name":"main.lua","source":"return 1"}], "sources": []
    }))).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let b = axum::body::to_bytes(res.into_body(), 1<<20).await.unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&b).unwrap()["id"].as_str().unwrap().to_string();
    assert_eq!(id.len(), 8);

    let res = app.router.clone().oneshot(Request::builder().uri(format!("/api/toys/{id}")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let b = axum::body::to_bytes(res.into_body(), 1<<20).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(v["title"], "Hi");
    assert_eq!(v["files"][0]["name"], "main.lua");
    assert_eq!(v["author"]["handle"], "ann");

    let res = app.router.clone().oneshot(authed("PUT", &format!("/api/toys/{id}"), &sid, serde_json::json!({
        "title": "Hi2", "files": [{"name":"main.lua","source":"return 2"}], "sources": []
    }))).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM toy_revisions WHERE toy_id=?").bind(&id).fetch_one(&app.state.pool).await.unwrap();
    assert_eq!(n, 2, "create + update each snapshot a revision");
}

#[tokio::test]
async fn update_by_non_author_forbidden() {
    let app = common::test_app().await;
    let owner = common::seed_session(&app.state, "1", "ann", false).await;
    let other = common::seed_session(&app.state, "2", "bob", false).await;
    let res = app.router.clone().oneshot(authed("POST", "/api/toys", &owner, serde_json::json!({"title":"x","files":[],"sources":[]}))).await.unwrap();
    let b = axum::body::to_bytes(res.into_body(), 1<<20).await.unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&b).unwrap()["id"].as_str().unwrap().to_string();
    let res = app.router.clone().oneshot(authed("PUT", &format!("/api/toys/{id}"), &other, serde_json::json!({"title":"y","files":[],"sources":[]}))).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn source_payload_roundtrips_through_get() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    // base64 of bytes [1,2,3,4] = "AQIDBA=="
    let res = app.router.clone().oneshot(authed("POST", "/api/toys", &sid, serde_json::json!({
        "title":"S","files":[],"sources":[{"name":"bg1","kind":"bg","payload":"AQIDBA==","options":{"a":1},"meta":{"w":8}}]
    }))).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let b = axum::body::to_bytes(res.into_body(), 1<<20).await.unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&b).unwrap()["id"].as_str().unwrap().to_string();
    let res = app.router.clone().oneshot(Request::builder().uri(format!("/api/toys/{id}")).body(Body::empty()).unwrap()).await.unwrap();
    let b = axum::body::to_bytes(res.into_body(), 1<<20).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(v["sources"][0]["name"], "bg1");
    assert_eq!(v["sources"][0]["payload"], "AQIDBA==");
    assert_eq!(v["sources"][0]["options"]["a"], 1);
}
