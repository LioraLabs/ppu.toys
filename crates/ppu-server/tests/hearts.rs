mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

fn heart(method: &str, id: &str, sid: &str) -> Request<Body> {
    Request::builder().method(method).uri(format!("/api/toys/{id}/heart"))
        .header("cookie", format!("ppu_sess={sid}")).header("x-ppu-csrf","1").body(Body::empty()).unwrap()
}

#[tokio::test]
async fn heart_is_idempotent_both_ways() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('t','1','T','[]','published',1)").execute(&app.state.pool).await.unwrap();

    for _ in 0..2 { assert_eq!(app.router.clone().oneshot(heart("PUT","t",&sid)).await.unwrap().status(), StatusCode::NO_CONTENT); }
    let (c,): (i64,) = sqlx::query_as("SELECT heart_count FROM toys WHERE id='t'").fetch_one(&app.state.pool).await.unwrap();
    assert_eq!(c, 1);
    for _ in 0..2 { assert_eq!(app.router.clone().oneshot(heart("DELETE","t",&sid)).await.unwrap().status(), StatusCode::NO_CONTENT); }
    let (c,): (i64,) = sqlx::query_as("SELECT heart_count FROM toys WHERE id='t'").fetch_one(&app.state.pool).await.unwrap();
    assert_eq!(c, 0);
}

#[tokio::test]
async fn heart_requires_csrf() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('t','1','T','[]','published',1)").execute(&app.state.pool).await.unwrap();
    let res = app.router.clone().oneshot(Request::builder().method("PUT").uri("/api/toys/t/heart")
        .header("cookie", format!("ppu_sess={sid}")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
