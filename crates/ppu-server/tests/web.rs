mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn t_route_injects_og_tags() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), "<head><!--OG--></head><body>app</body>").unwrap();
    let app = common::test_app_web(dir.path()).await;
    sqlx::query("INSERT INTO users(id,handle,avatar_hash,created_at) VALUES('1','ann','av',1)").execute(&app.state.pool).await.unwrap();
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('abc','1','My Toy','[]','published',1)").execute(&app.state.pool).await.unwrap();

    let res = app.router.clone().oneshot(Request::builder().uri("/t/abc").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let b = axum::body::to_bytes(res.into_body(), 1<<20).await.unwrap();
    let html = String::from_utf8(b.to_vec()).unwrap();
    assert!(html.contains("og:title"));
    assert!(html.contains("My Toy"));
    assert!(html.contains("/blobs/thumb/abc"));
}

#[tokio::test]
async fn t_route_escapes_html_in_title() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), "<head><!--OG--></head><body>app</body>").unwrap();
    let app = common::test_app_web(dir.path()).await;
    sqlx::query("INSERT INTO users(id,handle,created_at) VALUES('1','ann',1)").execute(&app.state.pool).await.unwrap();
    sqlx::query(r#"INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('abc','1','<script>x</script>','[]','published',1)"#).execute(&app.state.pool).await.unwrap();
    let res = app.router.clone().oneshot(Request::builder().uri("/t/abc").body(Body::empty()).unwrap()).await.unwrap();
    let b = axum::body::to_bytes(res.into_body(), 1<<20).await.unwrap();
    let html = String::from_utf8(b.to_vec()).unwrap();
    assert!(!html.contains("<script>x</script>"), "title must be HTML-escaped in the meta tag");
    assert!(html.contains("&lt;script&gt;"));
}

#[tokio::test]
async fn unknown_client_route_falls_back_to_index() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), "<head></head><body>spa</body>").unwrap();
    let app = common::test_app_web(dir.path()).await;
    let res = app.router.clone().oneshot(Request::builder().uri("/u/someone").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let b = axum::body::to_bytes(res.into_body(), 1<<20).await.unwrap();
    assert!(String::from_utf8(b.to_vec()).unwrap().contains("spa"));
}

#[tokio::test]
async fn static_asset_is_served() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), "<head></head><body>spa</body>").unwrap();
    std::fs::write(dir.path().join("app.js"), "console.log(1)").unwrap();
    let app = common::test_app_web(dir.path()).await;
    let res = app.router.clone().oneshot(Request::builder().uri("/app.js").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let b = axum::body::to_bytes(res.into_body(), 1<<20).await.unwrap();
    assert_eq!(String::from_utf8(b.to_vec()).unwrap(), "console.log(1)");
}
