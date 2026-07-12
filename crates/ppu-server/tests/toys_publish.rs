mod common;
use axum::body::Body;
use axum::http::{Request, StatusCode};
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
        body.extend_from_slice(data); body.extend(b"\r\n");
    };
    part("meta", None, "application/json", br#"{"title":"Published"}"#, &mut body);
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
    let res = app.router.clone().oneshot(Request::builder().method("POST").uri("/api/toys/t/publish")
        .header("cookie", format!("ppu_sess={sid}")).header("x-ppu-csrf","1").header("content-type", ct)
        .body(Body::from(body)).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let (st, clip): (String, Option<Vec<u8>>) = sqlx::query_as("SELECT state, clip FROM toys WHERE id='t'").fetch_one(&app.state.pool).await.unwrap();
    assert_eq!(st, "published");
    assert_eq!(clip.as_deref(), Some(&b"clipdata"[..]));
    let (title,): (String,) = sqlx::query_as("SELECT title FROM toys WHERE id='t'").fetch_one(&app.state.pool).await.unwrap();
    assert_eq!(title, "Published", "title updated from meta");
}

#[tokio::test]
async fn publish_rejects_oversized_clip() {
    let app = common::test_app().await;
    let sid = common::seed_session(&app.state, "1", "ann", false).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('t','1','T','[]','draft',1)").execute(&app.state.pool).await.unwrap();
    let big = vec![0u8; 2*1024*1024 + 1];
    let (ct, body) = multipart(&big, b"thumb");
    let res = app.router.clone().oneshot(Request::builder().method("POST").uri("/api/toys/t/publish")
        .header("cookie", format!("ppu_sess={sid}")).header("x-ppu-csrf","1").header("content-type", ct)
        .body(Body::from(body)).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn publish_by_non_author_forbidden() {
    let app = common::test_app().await;
    common::seed_session(&app.state, "1", "ann", false).await;
    let other = common::seed_session(&app.state, "2", "bob", false).await;
    sqlx::query("INSERT INTO toys(id,author_id,title,files_json,state,created_at) VALUES('t','1','T','[]','draft',1)").execute(&app.state.pool).await.unwrap();
    let (ct, body) = multipart(b"c", b"t");
    let res = app.router.clone().oneshot(Request::builder().method("POST").uri("/api/toys/t/publish")
        .header("cookie", format!("ppu_sess={other}")).header("x-ppu-csrf","1").header("content-type", ct)
        .body(Body::from(body)).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}
