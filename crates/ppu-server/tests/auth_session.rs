mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn me_401_without_cookie() {
    let app = common::test_app().await;
    let res = app.router.clone().oneshot(Request::builder().uri("/api/me").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_returns_user_with_valid_session() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "42", "neo", false).await;
    let res = app.router.clone().oneshot(
        Request::builder().uri("/api/me").header("cookie", format!("ppu_sess={sid}")).body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["handle"], "neo");
    assert_eq!(v["id"], "42");
}

#[tokio::test]
async fn mutating_route_needs_csrf_header() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "42", "neo", false).await;
    let res = app.router.clone().oneshot(
        Request::builder().method("POST").uri("/api/auth/logout").header("cookie", format!("ppu_sess={sid}")).body(Body::empty()).unwrap()
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
