mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn request(
    app: &common::TestApp,
    method: &str,
    uri: &str,
    sid: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("cookie", format!("ppu_sess={sid}"))
                .header("x-ppu-csrf", "1")
                .header("content-type", "application/json")
                .body(if method == "GET" {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn tags_normalize_round_trip_and_older_clients_preserve_them() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    let other = common::seed_session(&app.state, "2", "bob", false).await;
    let (status, created) = request(
        &app,
        "POST",
        "/api/toys",
        &sid,
        json!({"title":"Game", "files":[], "tags":[" Playable ","arcade","playable"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let url = format!("/api/toys/{}", created["id"].as_str().unwrap());
    let (_, toy) = request(&app, "GET", &url, &sid, Value::Null).await;
    assert_eq!(toy["tags"], json!(["playable", "arcade"]));
    let (status, _) = request(
        &app,
        "PUT",
        &url,
        &other,
        json!({"title":"Game", "files":[], "tags":[], "expectedRevision":1}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = request(
        &app,
        "PUT",
        &url,
        &sid,
        json!({"title":"Game", "files":[], "expectedRevision":1}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, toy) = request(&app, "GET", &url, &sid, Value::Null).await;
    assert_eq!(toy["tags"], json!(["playable", "arcade"]));
}

#[tokio::test]
async fn invalid_tags_do_not_write_and_explicit_empty_tags_clear() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    for tags in [
        json!(["bad tag"]),
        json!(["<script>"]),
        json!([""]),
        json!(["-bad"]),
        json!(["a".repeat(25)]),
        json!(["a", "b", "c", "d", "e", "f"]),
    ] {
        let (status, _) = request(
            &app,
            "POST",
            "/api/toys",
            &sid,
            json!({"title":"Game", "files":[], "tags":tags}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM toys")
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let (_, created) = request(
        &app,
        "POST",
        "/api/toys",
        &sid,
        json!({"title":"Game", "files":[], "tags":["playable"]}),
    )
    .await;
    let url = format!("/api/toys/{}", created["id"].as_str().unwrap());
    let (status, _) = request(
        &app,
        "PUT",
        &url,
        &sid,
        json!({"title":"Game", "files":[], "tags":[], "expectedRevision":1}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, toy) = request(&app, "GET", &url, &sid, Value::Null).await;
    assert_eq!(toy["tags"], json!([]));
}

#[tokio::test]
async fn tag_and_author_filters_paginate_only_matching_published_toys() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    common::seed_session(&app.state, "2", "bob", false).await;
    for n in 0..27 {
        sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at,tags_json) VALUES(?,?,'Game','[]',?,100,?)")
            .bind(format!("game{n:02}")).bind(if n == 26 { "2" } else { "1" })
            .bind(if n == 25 { "draft" } else { "published" })
            .bind(if n == 24 { "[\"unplayable\"]" } else { "[\"playable\"]" })
            .execute(&app.state.pool).await.unwrap();
    }
    let (_, first) = request(&app, "GET", "/api/toys?tag=playable", &sid, Value::Null).await;
    assert_eq!(first["toys"].as_array().unwrap().len(), 24);
    assert_eq!(first["nextPage"], 1);
    assert_eq!(first["toys"][0]["tags"], json!(["playable"]));
    let (_, second) = request(
        &app,
        "GET",
        "/api/toys?tag=playable&page=1",
        &sid,
        Value::Null,
    )
    .await;
    assert_eq!(second["toys"].as_array().unwrap().len(), 1);
    assert_eq!(second["nextPage"], Value::Null);
    assert!(!first["toys"]
        .as_array()
        .unwrap()
        .iter()
        .any(|toy| toy["id"] == second["toys"][0]["id"]));
    let (_, mine) = request(
        &app,
        "GET",
        "/api/toys?tag=playable&author=ann",
        &sid,
        Value::Null,
    )
    .await;
    assert_eq!(mine["toys"].as_array().unwrap().len(), 24);
    assert_eq!(mine["nextPage"], Value::Null);
    assert!(mine["toys"]
        .as_array()
        .unwrap()
        .iter()
        .all(|toy| toy["author"]["handle"] == "ann"));
    let (_, profile) = request(&app, "GET", "/api/users/ann", &sid, Value::Null).await;
    assert!(profile["toys"]
        .as_array()
        .unwrap()
        .iter()
        .all(|toy| toy["tags"].is_array()));
}
