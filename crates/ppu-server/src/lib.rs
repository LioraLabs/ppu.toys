pub mod admin;
pub mod auth;
pub mod blobs;
pub mod config;
pub mod db;
pub mod error;
pub mod hearts;
pub mod starter;
pub mod state;
pub mod toys;
pub mod web;

use axum::extract::DefaultBodyLimit;
use axum::routing::get;
use axum::Router;
use state::AppState;

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .route(
            "/health",
            get(|| async { axum::Json(serde_json::json!({ "ok": true })) }),
        )
        .merge(auth::routes())
        .merge(starter::routes())
        .merge(toys::routes())
        .merge(hearts::routes())
        .merge(admin::routes())
        // The only size limit is the 100MB per-account storage quota, checked in the
        // handlers; the body limit just has to admit a quota-sized toy plus base64
        // and multipart overhead.
        .layer(DefaultBodyLimit::max(
            crate::config::MAX_STORAGE_PER_USER as usize * 3 / 2,
        ));
    Router::new()
        .nest("/api", api)
        .merge(blobs::routes())
        .merge(web::routes(&state))
        .with_state(state)
}

pub async fn serve(state: AppState, listener: tokio::net::TcpListener) -> anyhow::Result<()> {
    axum::serve(listener, build_router(state)).await?;
    Ok(())
}
