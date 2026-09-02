mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

fn authed(method: &str, uri: &str, sid: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("cookie", format!("ppu_sess={sid}"))
        .header("x-ppu-csrf", "1")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn authed_get(uri: &str, sid: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("cookie", format!("ppu_sess={sid}"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn create_get_update_does_not_accumulate_hidden_snapshots() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;

    let res = app
        .router
        .clone()
        .oneshot(authed(
            "POST",
            "/api/toys",
            &sid,
            serde_json::json!({
                "title": "Hi", "files": [{"name":"main.lua","source":"return 1"}], "sources": []
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&b).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(id.len(), 8);

    let res = app
        .router
        .clone()
        .oneshot(authed_get(&format!("/api/toys/{id}"), &sid))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(v["title"], "Hi");
    assert_eq!(v["files"][0]["name"], "main.lua");
    assert_eq!(v["author"]["handle"], "ann");

    let res = app
        .router
        .clone()
        .oneshot(authed(
            "PUT",
            &format!("/api/toys/{id}"),
            &sid,
            serde_json::json!({
                "title": "Hi2", "files": [{"name":"main.lua","source":"return 2"}], "sources": [], "expectedRevision": 1
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM toy_revisions WHERE toy_id=?")
        .bind(&id)
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "saves must not accumulate unused full-file copies");
}

#[tokio::test]
async fn update_by_non_author_forbidden() {
    let app = common::test_app().await;
    let owner = common::seed_session(&app.state, "1", "ann", false).await;
    let other = common::seed_session(&app.state, "2", "bob", false).await;
    let res = app
        .router
        .clone()
        .oneshot(authed(
            "POST",
            "/api/toys",
            &owner,
            serde_json::json!({"title":"x","files":[],"sources":[]}),
        ))
        .await
        .unwrap();
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&b).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let res = app
        .router
        .clone()
        .oneshot(authed(
            "PUT",
            &format!("/api/toys/{id}"),
            &other,
            serde_json::json!({"title":"y","files":[],"sources":[]}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn stale_revision_is_conflict_without_overwriting() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('t','1','current','[]','draft',1)")
        .execute(&app.state.pool).await.unwrap();

    let res = app
        .router
        .clone()
        .oneshot(authed(
            "PUT",
            "/api/toys/t",
            &sid,
            serde_json::json!({"title":"next","files":[],"sources":[],"expectedRevision":1}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .router
        .clone()
        .oneshot(authed(
            "PUT",
            "/api/toys/t",
            &sid,
            serde_json::json!({"title":"stale","files":[],"sources":[],"expectedRevision":1}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let (title, revision): (String, i64) =
        sqlx::query_as("SELECT title,revision FROM toys WHERE id='t'")
            .fetch_one(&app.state.pool)
            .await
            .unwrap();
    assert_eq!((title.as_str(), revision), ("next", 2));
}

#[tokio::test]
async fn source_payload_roundtrips_through_get() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    // base64 of bytes [1,2,3,4] = "AQIDBA=="
    let res = app.router.clone().oneshot(authed("POST", "/api/toys", &sid, serde_json::json!({
        "title":"S","files":[],"sources":[{"name":"bg1","kind":"bg","payload":"AQIDBA==","options":{"a":1},"meta":{"w":8}}]
    }))).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&b).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let res = app
        .router
        .clone()
        .oneshot(authed_get(&format!("/api/toys/{id}"), &sid))
        .await
        .unwrap();
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(v["sources"][0]["name"], "bg1");
    assert_eq!(v["sources"][0]["payload"], "AQIDBA==");
    assert_eq!(v["sources"][0]["options"]["a"], 1);
}

#[tokio::test]
async fn oversized_lua_file_rejected_413() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    let big = "x".repeat(64 * 1024 + 1);
    let res = app
        .router
        .clone()
        .oneshot(authed(
            "POST",
            "/api/toys",
            &sid,
            serde_json::json!({
                "title":"B","files":[{"name":"m.lua","source":big}],"sources":[]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM toys")
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "rejected before any toy row is written");
}

#[tokio::test]
async fn oversized_source_payload_rejected_413() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    use base64::Engine;
    let payload = base64::engine::general_purpose::STANDARD.encode(vec![0u8; 128 * 1024 + 1]);
    let res = app
        .router
        .clone()
        .oneshot(authed(
            "POST",
            "/api/toys",
            &sid,
            serde_json::json!({
                "title":"B","files":[],"sources":[{"name":"bg","kind":"bg","payload":payload}]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM toys")
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "rejected before any toy row is written");
}

#[tokio::test]
async fn toy_exceeding_total_cap_rejected_413() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    use base64::Engine;
    // 9 sources * 120KB each = 1.08MB > 1MB total, though each is under the 128KB per-source cap
    let sources: Vec<serde_json::Value> = (0..9)
        .map(|i| {
            serde_json::json!({
                "name": format!("s{i}"), "kind": "bg",
                "payload": base64::engine::general_purpose::STANDARD.encode(vec![0u8; 120 * 1024]),
            })
        })
        .collect();
    let res = app
        .router
        .clone()
        .oneshot(authed(
            "POST",
            "/api/toys",
            &sid,
            serde_json::json!({
                "title":"Big","files":[],"sources":sources
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM toys")
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(n, 0, "aggregate-cap rejection writes nothing");
}

#[tokio::test]
async fn builtin_id_round_trips_as_camel_case() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    // write side accepts camelCase `builtinId` (symmetric with the read side)
    let res = app.router.clone().oneshot(authed("POST", "/api/toys", &sid, serde_json::json!({
        "title":"B","files":[],"sources":[{"name":"logo","kind":"builtin","builtinId":"mode7-photo"}]
    }))).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&b).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let res = app
        .router
        .clone()
        .oneshot(authed_get(&format!("/api/toys/{id}"), &sid))
        .await
        .unwrap();
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(v["sources"][0]["builtinId"], "mode7-photo");
}

#[tokio::test]
async fn drafts_are_visible_only_to_their_owner() {
    let app = common::test_app().await;
    let owner = common::seed_session(&app.state, "1", "ann", false).await;
    let other = common::seed_session(&app.state, "2", "bob", false).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('draft','1','D','[]','draft',1)")
        .execute(&app.state.pool).await.unwrap();

    let anonymous = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/toys/draft")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let stranger = app
        .router
        .clone()
        .oneshot(authed_get("/api/toys/draft", &other))
        .await
        .unwrap();
    let owner_response = app
        .router
        .clone()
        .oneshot(authed_get("/api/toys/draft", &owner))
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::NOT_FOUND);
    assert_eq!(stranger.status(), StatusCode::NOT_FOUND);
    assert_eq!(owner_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn create_enforces_input_bounds_and_per_user_quota() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    let duplicate = app.router.clone().oneshot(authed("POST", "/api/toys", &sid, serde_json::json!({
        "title":"x", "files":[{"name":"main.lua","source":""},{"name":"main.lua","source":""}], "sources":[]
    }))).await.unwrap();
    assert_eq!(duplicate.status(), StatusCode::BAD_REQUEST);

    let now = ppu_server::db::now();
    for i in 0..ppu_server::config::MAX_TOYS_PER_USER {
        sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES(?, '1','T','[]','draft',?)")
            .bind(format!("t{i}"))
            .bind(now)
            .execute(&app.state.pool).await.unwrap();
    }
    let over_quota = app
        .router
        .clone()
        .oneshot(authed(
            "POST",
            "/api/toys",
            &sid,
            serde_json::json!({
                "title":"x", "files":[], "sources":[]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(over_quota.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn update_replaces_source_set() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    let res = app
        .router
        .clone()
        .oneshot(authed(
            "POST",
            "/api/toys",
            &sid,
            serde_json::json!({
                "title":"S","files":[],"sources":[{"name":"a","kind":"bg"},{"name":"b","kind":"bg"}]
            }),
        ))
        .await
        .unwrap();
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&b).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    // update dropping source "b"
    let res = app
        .router
        .clone()
        .oneshot(authed(
            "PUT",
            &format!("/api/toys/{id}"),
            &sid,
            serde_json::json!({
                "title":"S","files":[],"sources":[{"name":"a","kind":"bg"}],"expectedRevision":1
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let names: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM toy_sources WHERE toy_id=? ORDER BY name")
            .bind(&id)
            .fetch_all(&app.state.pool)
            .await
            .unwrap();
    assert_eq!(
        names.iter().map(|(n,)| n.as_str()).collect::<Vec<_>>(),
        vec!["a"],
        "dropped source is removed, not left stale"
    );
}

#[tokio::test]
async fn create_with_forked_from_stores_it() {
    let app = common::test_app().await;
    let _owner = common::seed_session(&app.state, "1", "ann", false).await;
    let forker = common::seed_session(&app.state, "2", "bob", false).await;
    let now = ppu_server::db::now();
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('orig','1','Orig','[]','published',?)")
        .bind(now)
        .execute(&app.state.pool).await.unwrap();

    let res = app
        .router
        .clone()
        .oneshot(authed(
            "POST",
            "/api/toys",
            &forker,
            serde_json::json!({
                "title":"f","files":[],"sources":[],"forkedFrom":"orig"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let id = serde_json::from_slice::<serde_json::Value>(&b).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let res = app
        .router
        .clone()
        .oneshot(authed_get(&format!("/api/toys/{id}"), &forker))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let b = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(v["forkedFrom"], "orig");
}

#[tokio::test]
async fn forked_from_missing_or_unpublished_is_404() {
    let app = common::test_app().await;
    let _owner = common::seed_session(&app.state, "1", "ann", false).await;
    let forker = common::seed_session(&app.state, "2", "bob", false).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('unpub','1','Unpub','[]','draft',?)")
        .bind(ppu_server::db::now())
        .execute(&app.state.pool).await.unwrap();

    let res = app
        .router
        .clone()
        .oneshot(authed(
            "POST",
            "/api/toys",
            &forker,
            serde_json::json!({
                "title":"f","files":[],"sources":[],"forkedFrom":"nope"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let res = app
        .router
        .clone()
        .oneshot(authed(
            "POST",
            "/api/toys",
            &forker,
            serde_json::json!({
                "title":"f","files":[],"sources":[],"forkedFrom":"unpub"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_sweeps_callers_stale_drafts() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    let _ = common::seed_session(&app.state, "2", "bob", false).await;
    let now = ppu_server::db::now();
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('old','1','Old','[]','draft',?)")
        .bind(now - 2 * ppu_server::config::DRAFT_SWEEP_SECS)
        .execute(&app.state.pool).await.unwrap();
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('fresh','1','Fresh','[]','draft',?)")
        .bind(now - ppu_server::config::DRAFT_SWEEP_SECS / 2)
        .execute(&app.state.pool).await.unwrap();
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('pub','1','Pub','[]','published',?)")
        .bind(now - 2 * ppu_server::config::DRAFT_SWEEP_SECS)
        .execute(&app.state.pool).await.unwrap();
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('other_old','2','OtherOld','[]','draft',?)")
        .bind(now - 2 * ppu_server::config::DRAFT_SWEEP_SECS)
        .execute(&app.state.pool).await.unwrap();
    sqlx::query("INSERT INTO toy_sources(toy_id,name,kind) VALUES('old','s','bg')")
        .execute(&app.state.pool)
        .await
        .unwrap();

    let res = app
        .router
        .clone()
        .oneshot(authed(
            "POST",
            "/api/toys",
            &sid,
            serde_json::json!({
                "title":"n","files":[],"sources":[]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let ids: Vec<(String,)> = sqlx::query_as("SELECT id FROM toys ORDER BY id")
        .fetch_all(&app.state.pool)
        .await
        .unwrap();
    let ids: Vec<&str> = ids.iter().map(|(i,)| i.as_str()).collect();
    assert!(
        !ids.contains(&"old"),
        "stale draft should be swept: {ids:?}"
    );
    assert!(ids.contains(&"fresh"));
    assert!(ids.contains(&"pub"));
    assert!(ids.contains(&"other_old"));

    let sources: i64 =
        sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM toy_sources WHERE toy_id='old'")
            .fetch_one(&app.state.pool)
            .await
            .unwrap()
            .0;
    assert_eq!(sources, 0, "swept draft's toy_sources row should cascade");
}

#[tokio::test]
async fn stale_drafts_do_not_eat_the_quota() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    let now = ppu_server::db::now();
    for i in 0..ppu_server::config::MAX_TOYS_PER_USER {
        sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES(?, '1','T','[]','draft',?)")
            .bind(format!("d{i}"))
            .bind(now - 2 * ppu_server::config::DRAFT_SWEEP_SECS)
            .execute(&app.state.pool).await.unwrap();
    }

    let res = app
        .router
        .clone()
        .oneshot(authed(
            "POST",
            "/api/toys",
            &sid,
            serde_json::json!({
                "title":"x","files":[],"sources":[]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
