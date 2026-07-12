mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn admin_can_delete_any_toy_but_user_cannot() {
    let app = common::test_app().await;
    let admin = common::seed_session(&app.state, "9", "root", true).await;
    let user = common::seed_session(&app.state, "1", "ann", false).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('t','1','T','[]','published',1)").execute(&app.state.pool).await.unwrap();

    let del = |sid: &str| app.router.clone().oneshot(Request::builder().method("DELETE").uri("/api/admin/toys/t")
        .header("cookie", format!("ppu_sess={sid}")).header("x-ppu-csrf","1").body(Body::empty()).unwrap());
    assert_eq!(del(&user).await.unwrap().status(), StatusCode::FORBIDDEN);
    assert_eq!(del(&admin).await.unwrap().status(), StatusCode::NO_CONTENT);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM toys WHERE id='t'").fetch_one(&app.state.pool).await.unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn admin_ban_inserts_and_deletes_sessions() {
    let app = common::test_app().await;
    let admin = common::seed_session(&app.state, "9", "root", true).await;
    common::seed_session(&app.state, "1", "ann", false).await;
    let res = app.router.clone().oneshot(Request::builder().method("POST").uri("/api/admin/ban")
        .header("cookie", format!("ppu_sess={admin}")).header("x-ppu-csrf","1").header("content-type","application/json")
        .body(Body::from(r#"{"discord_id":"1"}"#)).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM bans WHERE discord_id='1'").fetch_one(&app.state.pool).await.unwrap();
    assert_eq!(n, 1);
    let (s,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id='1'").fetch_one(&app.state.pool).await.unwrap();
    assert_eq!(s, 0, "ban revokes sessions");
}

#[tokio::test]
async fn non_admin_cannot_ban() {
    let app = common::test_app().await;
    let user = common::seed_session(&app.state, "1", "ann", false).await;
    let res = app.router.clone().oneshot(Request::builder().method("POST").uri("/api/admin/ban")
        .header("cookie", format!("ppu_sess={user}")).header("x-ppu-csrf","1").header("content-type","application/json")
        .body(Body::from(r#"{"discord_id":"2"}"#)).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
