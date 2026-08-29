mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

async fn seed_published(
    state: &ppu_server::state::AppState,
    id: &str,
    author: &str,
    hearts: i64,
    created: i64,
) {
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,heart_count,created_at,published_at) VALUES(?,?,?,'[]','published',?,?,?)")
        .bind(id).bind(author).bind(id).bind(hearts).bind(created).bind(created).execute(&state.pool).await.unwrap();
}

#[tokio::test]
async fn wall_recent_and_popular_sorts() {
    let app = common::test_app().await;
    common::seed_session(&app.state, "1", "ann", false).await;
    seed_published(&app.state, "aaaaaaaa", "1", 1, 100).await;
    seed_published(&app.state, "bbbbbbbb", "1", 9, 200).await;

    let get = |uri: &str| {
        app.router.clone().oneshot(
            Request::builder()
                .uri(uri.to_string())
                .body(Body::empty())
                .unwrap(),
        )
    };
    let res = get("/api/toys?sort=recent").await.unwrap();
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(v["toys"][0]["id"], "bbbbbbbb");
    assert_eq!(v["toys"][0]["thumbUrl"], "/blobs/thumb/bbbbbbbb");
    assert_eq!(v["toys"][0]["clipUrl"], "/blobs/clip/bbbbbbbb");

    let res = get("/api/toys?sort=popular").await.unwrap();
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(v["toys"][0]["id"], "bbbbbbbb");

    let res = get("/api/toys?q=aaaaaaaa").await.unwrap();
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(v["toys"].as_array().unwrap().len(), 1);
    assert_eq!(v["toys"][0]["id"], "aaaaaaaa");
}

#[tokio::test]
async fn wall_excludes_drafts() {
    let app = common::test_app().await;
    common::seed_session(&app.state, "1", "ann", false).await;
    seed_published(&app.state, "pppppppp", "1", 0, 100).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('dddddddd','1','d','[]','draft',150)").execute(&app.state.pool).await.unwrap();
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/toys?sort=recent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    let ids: Vec<&str> = v["toys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["pppppppp"], "drafts must not appear on the wall");
}

#[tokio::test]
async fn profile_lists_only_that_users_published_toys() {
    let app = common::test_app().await;
    common::seed_session(&app.state, "1", "ann", false).await;
    common::seed_session(&app.state, "2", "bob", false).await;
    seed_published(&app.state, "aaaaaaaa", "1", 0, 100).await;
    seed_published(&app.state, "bbbbbbbb", "2", 0, 100).await;
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users/ann")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(v["user"]["handle"], "ann");
    assert_eq!(v["toys"].as_array().unwrap().len(), 1);
    assert_eq!(v["toys"][0]["id"], "aaaaaaaa");
}

#[tokio::test]
async fn profile_drafts_are_owner_only() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    common::seed_session(&app.state, "2", "bob", false).await;
    seed_published(&app.state, "aaaaaaaa", "1", 0, 100).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('dddddddd','1','wip','[]','draft',150)")
        .execute(&app.state.pool).await.unwrap();

    // owner: drafts present, newest first
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users/ann")
                .header("cookie", format!("ppu_sess={sid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(v["drafts"][0]["id"], "dddddddd");
    assert_eq!(v["drafts"][0]["title"], "wip");

    // signed-out viewer: no drafts key at all
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users/ann")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert!(v.get("drafts").is_none());

    // another signed-in viewer: still none
    let sid2 = "sess_2";
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/users/ann")
                .header("cookie", format!("ppu_sess={sid2}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert!(v.get("drafts").is_none());
}
