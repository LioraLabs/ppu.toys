mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn admin_selects_a_published_featured_toy() {
    let app = common::test_app().await;
    let admin = common::seed_session(&app.state, "9", "root", true).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('featured','9','Feature','[]','published',1)")
        .execute(&app.state.pool).await.unwrap();
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/admin/featured")
                .header("cookie", format!("ppu_sess={admin}"))
                .header("x-ppu-csrf", "1")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"toy_id":"featured"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/featured")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["id"],
        "featured"
    );
}

#[tokio::test]
async fn admin_selects_ordered_community_highlights() {
    let app = common::test_app().await;
    let admin = common::seed_session(&app.state, "9", "root", true).await;
    for (id, title) in [("one", "One"), ("two", "Two")] {
        sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES(?, '9', ?, '[]', 'published', 1)")
            .bind(id).bind(title).execute(&app.state.pool).await.unwrap();
    }
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/admin/featured-toys")
                .header("cookie", format!("ppu_sess={admin}"))
                .header("x-ppu-csrf", "1")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"toy_ids":["two","one"]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/highlights")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["toys"][0]["id"], "two");
    assert_eq!(json["toys"][1]["id"], "one");
}

#[tokio::test]
async fn admin_overview_lists_users_and_toys_and_can_unban() {
    let app = common::test_app().await;
    let admin = common::seed_session(&app.state, "9", "root", true).await;
    common::seed_session(&app.state, "1", "ann", false).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('t','1','Toy','[]','draft',1)")
        .execute(&app.state.pool).await.unwrap();
    sqlx::query("INSERT INTO bans(discord_id,created_at) VALUES('1',1)")
        .execute(&app.state.pool)
        .await
        .unwrap();

    let overview = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/admin")
                .header("cookie", format!("ppu_sess={admin}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(overview.status(), StatusCode::OK);
    let body = axum::body::to_bytes(overview.into_body(), 64 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["toys"][0]["title"], "Toy");
    assert!(json["storage"]["usedBytes"].as_i64().unwrap() > 0);
    assert_eq!(json["storage"]["warning"], false);
    assert_eq!(json["featuredToyId"], "");
    assert!(json["users"]
        .as_array()
        .unwrap()
        .iter()
        .any(|u| u["handle"] == "ann"
            && u["banned"] == true
            && u["storage_bytes"].as_i64().is_some()));

    let unban = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/admin/ban/1")
                .header("cookie", format!("ppu_sess={admin}"))
                .header("x-ppu-csrf", "1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unban.status(), StatusCode::NO_CONTENT);
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM bans WHERE discord_id='1'")
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn admin_can_delete_any_toy_but_user_cannot() {
    let app = common::test_app().await;
    let admin = common::seed_session(&app.state, "9", "root", true).await;
    let user = common::seed_session(&app.state, "1", "ann", false).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('t','1','T','[]','published',1)").execute(&app.state.pool).await.unwrap();

    let del = |sid: &str| {
        app.router.clone().oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/admin/toys/t")
                .header("cookie", format!("ppu_sess={sid}"))
                .header("x-ppu-csrf", "1")
                .body(Body::empty())
                .unwrap(),
        )
    };
    assert_eq!(del(&user).await.unwrap().status(), StatusCode::FORBIDDEN);
    assert_eq!(del(&admin).await.unwrap().status(), StatusCode::NO_CONTENT);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM toys WHERE id='t'")
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn admin_delete_detaches_forks_and_cascades_children() {
    let app = common::test_app().await;
    let admin = common::seed_session(&app.state, "9", "root", true).await;
    common::seed_session(&app.state, "1", "ann", false).await;
    // original + a fork of it + dependent rows on the original
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('orig','1','O','[]','published',1)").execute(&app.state.pool).await.unwrap();
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,forked_from,created_at) VALUES('fork','1','F','[]','draft','orig',2)").execute(&app.state.pool).await.unwrap();
    sqlx::query("INSERT INTO toy_sources(toy_id,name,kind) VALUES('orig','s','bg')")
        .execute(&app.state.pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO hearts(user_id,toy_id,created_at) VALUES('1','orig',1)")
        .execute(&app.state.pool)
        .await
        .unwrap();

    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/admin/toys/orig")
                .header("cookie", format!("ppu_sess={admin}"))
                .header("x-ppu-csrf", "1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::NO_CONTENT,
        "deleting a forked toy must not FK-fail"
    );

    let (toys,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM toys WHERE id='orig'")
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(toys, 0);
    let (srcs,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM toy_sources WHERE toy_id='orig'")
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(srcs, 0, "child sources cascade-deleted");
    let (hearts,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM hearts WHERE toy_id='orig'")
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(hearts, 0, "child hearts cascade-deleted");
    let (fk,): (Option<String>,) = sqlx::query_as("SELECT forked_from FROM toys WHERE id='fork'")
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(fk, None, "fork detached, fork row survives");
}

#[tokio::test]
async fn admin_ban_inserts_and_deletes_sessions() {
    let app = common::test_app().await;
    let admin = common::seed_session(&app.state, "9", "root", true).await;
    common::seed_session(&app.state, "1", "ann", false).await;
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/admin/ban")
                .header("cookie", format!("ppu_sess={admin}"))
                .header("x-ppu-csrf", "1")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"discord_id":"1"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM bans WHERE discord_id='1'")
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(n, 1);
    let (s,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id='1'")
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(s, 0, "ban revokes sessions");
}

#[tokio::test]
async fn non_admin_cannot_ban() {
    let app = common::test_app().await;
    let user = common::seed_session(&app.state, "1", "ann", false).await;
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/admin/ban")
                .header("cookie", format!("ppu_sess={user}"))
                .header("x-ppu-csrf", "1")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"discord_id":"2"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
