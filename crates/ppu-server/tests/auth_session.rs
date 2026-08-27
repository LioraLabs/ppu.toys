mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn expired_session_is_401() {
    let app = common::test_app().await;
    let now = ppu_server::db::now();
    sqlx::query("INSERT INTO users(id,handle,created_at) VALUES('7','morpheus',?)")
        .bind(now)
        .execute(&app.state.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sessions(id,user_id,created_at,expires_at) VALUES('expired','7',?,?)")
        .bind(now - 100)
        .bind(now - 1)
        .execute(&app.state.pool)
        .await
        .unwrap();
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header("cookie", "ppu_sess=expired")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_401_without_cookie() {
    let app = common::test_app().await;
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_returns_user_with_valid_session() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "42", "neo", false).await;
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header("cookie", format!("ppu_sess={sid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["handle"], "neo");
    assert_eq!(v["id"], "42");
}

#[tokio::test]
async fn mutating_route_needs_csrf_header() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "42", "neo", false).await;
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/logout")
                .header("cookie", format!("ppu_sess={sid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn personal_token_authenticates_without_csrf_and_is_stored_hashed() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "42", "neo", false).await;
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/tokens")
                .header("cookie", format!("ppu_sess={sid}"))
                .header("x-ppu-csrf", "1")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":"laptop"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
    let token = serde_json::from_slice::<serde_json::Value>(&body).unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let (stored,): (String,) = sqlx::query_as("SELECT token_hash FROM api_tokens")
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert!(!stored.contains(&token));

    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/me")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/toys")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"local","files":[],"sources":[]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
