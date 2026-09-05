mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use axum::Router;
use ppu_server::config::{BlobMode, DiscordConfig};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tower::ServiceExt;

fn multipart(clip: &[u8], thumb: &[u8]) -> (String, Vec<u8>) {
    let b = "BOUNDARY123";
    let mut body = Vec::new();
    let part = |name: &str, filename: Option<&str>, ct: &str, data: &[u8], body: &mut Vec<u8>| {
        body.extend(format!("--{b}\r\n").bytes());
        match filename {
            Some(fname) => body.extend(format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{fname}\"\r\nContent-Type: {ct}\r\n\r\n").bytes()),
            None => body.extend(format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").bytes()),
        }
        body.extend_from_slice(data);
        body.extend(b"\r\n");
    };
    part(
        "meta",
        None,
        "application/json",
        br#"{"title":"Published"}"#,
        &mut body,
    );
    part("clip", Some("c.webm"), "video/webm", clip, &mut body);
    part("thumb", Some("t.png"), "image/png", thumb, &mut body);
    body.extend(format!("--{b}--\r\n").bytes());
    (format!("multipart/form-data; boundary={b}"), body)
}

#[tokio::test]
async fn publish_flips_state_and_stores_blobs_webhook_skipped() {
    let app = common::test_app().await; // no discord => webhook skipped, must still succeed
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('t','1','T','[]','draft',1)").execute(&app.state.pool).await.unwrap();
    let (ct, body) = multipart(b"clipdata", b"thumbdata");
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/toys/t/publish")
                .header("cookie", format!("ppu_sess={sid}"))
                .header("x-ppu-csrf", "1")
                .header("content-type", ct)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let (st, clip): (String, Option<Vec<u8>>) =
        sqlx::query_as("SELECT state, clip FROM toys WHERE id='t'")
            .fetch_one(&app.state.pool)
            .await
            .unwrap();
    assert_eq!(st, "published");
    assert_eq!(clip.as_deref(), Some(&b"clipdata"[..]));
    let (title,): (String,) = sqlx::query_as("SELECT title FROM toys WHERE id='t'")
        .fetch_one(&app.state.pool)
        .await
        .unwrap();
    assert_eq!(title, "Published", "title updated from meta");
}

#[tokio::test]
async fn publish_by_non_author_forbidden() {
    let app = common::test_app().await;
    common::seed_session(&app.state, "1", "ann", false).await;
    let other = common::seed_session(&app.state, "2", "bob", false).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('t','1','T','[]','draft',1)").execute(&app.state.pool).await.unwrap();
    let (ct, body) = multipart(b"c", b"t");
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/toys/t/publish")
                .header("cookie", format!("ppu_sess={other}"))
                .header("x-ppu-csrf", "1")
                .header("content-type", ct)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn publish_is_not_capped_per_day() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    for n in 0..11 {
        sqlx::query(
            "INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES(?,'1','T','[]','draft',1)",
        )
        .bind(format!("t{n}"))
        .execute(&app.state.pool)
        .await
        .unwrap();
    }
    for n in 0..11 {
        let (ct, body) = multipart(b"c", b"t");
        let res = app
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/toys/t{n}/publish"))
                    .header("cookie", format!("ppu_sess={sid}"))
                    .header("x-ppu-csrf", "1")
                    .header("content-type", ct)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "toy t{n} should publish");
    }
}

fn discord_cfg(base: &str) -> DiscordConfig {
    DiscordConfig {
        client_id: "cid".into(),
        client_secret: "sec".into(),
        redirect_uri: "http://test.local/api/auth/callback".into(),
        authorize_url: "https://discord.com/oauth2/authorize".into(),
        token_url: format!("{base}/token"),
        userinfo_url: format!("{base}/users/@me"),
        webhook_url: Some(format!("{base}/hook")),
    }
}

async fn mock_discord(counter: Arc<AtomicUsize>) -> String {
    let app = Router::new().route(
        "/hook",
        post(move || {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                StatusCode::NO_CONTENT
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    base
}

#[tokio::test]
async fn republish_keeps_published_at_and_announces_once() {
    let counter = Arc::new(AtomicUsize::new(0));
    let base = mock_discord(counter.clone()).await;
    let app = common::test_app_with(Some(discord_cfg(&base)), BlobMode::Db).await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('t','1','T','[]','draft',1)").execute(&app.state.pool).await.unwrap();

    let (ct, body) = multipart(b"one", b"thumb");
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/toys/t/publish")
                .header("cookie", format!("ppu_sess={sid}"))
                .header("x-ppu-csrf", "1")
                .header("content-type", ct)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    for _ in 0..500 {
        if counter.load(Ordering::SeqCst) >= 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(counter.load(Ordering::SeqCst), 1, "first publish announces");

    let (published_at,): (Option<i64>,) =
        sqlx::query_as("SELECT published_at FROM toys WHERE id='t'")
            .fetch_one(&app.state.pool)
            .await
            .unwrap();
    let published_at = published_at.expect("published_at set on first publish");

    let (ct, body) = multipart(b"two", b"thumb2");
    let res = app
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/toys/t/publish")
                .header("cookie", format!("ppu_sess={sid}"))
                .header("x-ppu-csrf", "1")
                .header("content-type", ct)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let (clip, published_at2): (Option<Vec<u8>>, Option<i64>) =
        sqlx::query_as("SELECT clip, published_at FROM toys WHERE id='t'")
            .fetch_one(&app.state.pool)
            .await
            .unwrap();
    assert_eq!(
        clip.as_deref(),
        Some(&b"two"[..]),
        "republish replaces clip"
    );
    assert_eq!(
        published_at2,
        Some(published_at),
        "republish keeps published_at"
    );

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "republish does not re-announce"
    );
}
