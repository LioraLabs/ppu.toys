mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use ppu_server::state::RateLimiter;
use tower::ServiceExt;

#[test]
fn save_limited_to_one_per_minute() {
    let rl = RateLimiter::default();
    assert!(rl.check_save("u"));
    assert!(!rl.check_save("u"), "second save within a minute blocked");
    assert!(rl.check_save("other"), "per-user, not global");
}

async fn create_toy(app: &common::TestApp, auth_header: (&str, &str), csrf: bool) -> (String, i64) {
    let mut req = Request::builder()
        .method("POST")
        .uri("/api/toys")
        .header(auth_header.0, auth_header.1)
        .header("content-type", "application/json");
    if csrf {
        req = req.header("x-ppu-csrf", "1");
    }
    let res = app
        .router
        .clone()
        .oneshot(
            req.body(Body::from(r#"{"title":"t","files":[],"sources":[]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 4096).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    (
        v["id"].as_str().unwrap().to_string(),
        v["revision"].as_i64().unwrap(),
    )
}

async fn put_toy(
    app: &common::TestApp,
    auth_header: (&str, &str),
    csrf: bool,
    id: &str,
    expected_revision: i64,
) -> StatusCode {
    let mut req = Request::builder()
        .method("PUT")
        .uri(format!("/api/toys/{id}"))
        .header(auth_header.0, auth_header.1)
        .header("content-type", "application/json");
    if csrf {
        req = req.header("x-ppu-csrf", "1");
    }
    let res = app
        .router
        .clone()
        .oneshot(
            req.body(Body::from(format!(
                r#"{{"title":"t","files":[],"sources":[],"expectedRevision":{expected_revision}}}"#
            )))
            .unwrap(),
        )
        .await
        .unwrap();
    res.status()
}

#[tokio::test]
async fn browser_session_second_update_within_a_minute_is_429() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    let cookie = format!("ppu_sess={sid}");
    let (id, revision) = create_toy(&app, ("cookie", &cookie), true).await;
    assert_eq!(revision, 1);
    let status = put_toy(&app, ("cookie", &cookie), true, &id, 1).await;
    assert_eq!(status, StatusCode::OK);
    let status = put_toy(&app, ("cookie", &cookie), true, &id, 2).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}
