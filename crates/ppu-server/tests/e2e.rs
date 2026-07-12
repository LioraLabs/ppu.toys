mod common;
use ppu_server::config::Config;
use ppu_server::{build_router, db, state::AppState};

#[tokio::test]
async fn boot_seed_save_publish_wall() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::from_map(|_| None);
    cfg.db_path = dir.path().join("e2e.db").to_str().unwrap().into();
    cfg.web_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web-placeholder");
    cfg.base_url = "http://127.0.0.1".into();
    let pool = db::connect(&cfg.db_path).await.unwrap();
    db::migrate(&pool).await.unwrap();
    let state = AppState::new(cfg, pool);

    let sid = common::seed_session(&state, "1", "ann", false).await; // stub signin

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = build_router(state.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });

    let http = reqwest::Client::new();
    let auth = |r: reqwest::RequestBuilder| r.header("cookie", format!("ppu_sess={sid}")).header("x-ppu-csrf","1");

    assert!(http.get(format!("{base}/api/health")).send().await.unwrap().status().is_success());

    let created: serde_json::Value = auth(http.post(format!("{base}/api/toys")))
        .json(&serde_json::json!({"title":"E2E","files":[{"name":"m.lua","source":"return 1"}],"sources":[]}))
        .send().await.unwrap().json().await.unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    let form = reqwest::multipart::Form::new()
        .text("meta", r#"{"title":"E2E"}"#)
        .part("clip", reqwest::multipart::Part::bytes(b"clip".to_vec()).file_name("c.webm").mime_str("video/webm").unwrap())
        .part("thumb", reqwest::multipart::Part::bytes(b"thmb".to_vec()).file_name("t.png").mime_str("image/png").unwrap());
    let pub_res = auth(http.post(format!("{base}/api/toys/{id}/publish"))).multipart(form).send().await.unwrap();
    assert!(pub_res.status().is_success(), "publish failed: {}", pub_res.status());

    let wall: serde_json::Value = http.get(format!("{base}/api/toys?sort=recent")).send().await.unwrap().json().await.unwrap();
    let ids: Vec<String> = wall["toys"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap().to_string()).collect();
    assert!(ids.contains(&id), "published toy missing from wall");

    let html = http.get(format!("{base}/t/{id}")).send().await.unwrap().text().await.unwrap();
    assert!(html.contains("og:title") && html.contains("E2E"));

    // blob is served with a long cache header
    let clip = http.get(format!("{base}/blobs/clip/{id}")).send().await.unwrap();
    assert!(clip.headers().get("cache-control").unwrap().to_str().unwrap().contains("max-age"));
    assert_eq!(clip.bytes().await.unwrap().as_ref(), b"clip");
}
