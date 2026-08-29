mod common;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn starter_is_public_and_admin_configurable() {
    let app = common::test_app().await;
    let initial = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/starter")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(initial.status(), StatusCode::OK);
    let body = to_bytes(initial.into_body(), 64 * 1024).await.unwrap();
    assert!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["files"][0]["source"]
            .as_str()
            .unwrap()
            .contains("function frame")
    );

    let admin = common::seed_session(&app.state, "9", "root", true).await;
    let updated =
        r#"{"name":"hello","files":[{"name":"main.lua","source":"function frame() end"}]}"#;
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/starter")
                .header("cookie", format!("ppu_sess={admin}"))
                .header("x-ppu-csrf", "1")
                .header("content-type", "application/json")
                .body(Body::from(updated))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let (stored,): (String,) =
        sqlx::query_as("SELECT value FROM settings WHERE key='starter_template'")
            .fetch_one(&app.state.pool)
            .await
            .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&stored).unwrap()["name"],
        "hello"
    );
}
