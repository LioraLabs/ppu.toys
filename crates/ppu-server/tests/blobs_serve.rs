mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn clip_served_with_cache_header_and_404_when_absent() {
    let app = common::test_app().await;
    sqlx::query("INSERT INTO users(id,handle,created_at) VALUES('1','a',1)").execute(&app.state.pool).await.unwrap();
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,created_at,clip) VALUES('t','1','T','[]',1,?)").bind(&b"CLIP"[..]).execute(&app.state.pool).await.unwrap();

    let res = app.router.clone().oneshot(Request::builder().uri("/blobs/clip/t").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers().get("cache-control").unwrap().to_str().unwrap().contains("max-age"));
    let b = axum::body::to_bytes(res.into_body(), 1<<20).await.unwrap();
    assert_eq!(&b[..], b"CLIP");

    let res = app.router.clone().oneshot(Request::builder().uri("/blobs/thumb/t").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
