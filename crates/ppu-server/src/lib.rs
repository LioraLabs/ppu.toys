pub mod config;
pub mod db;
pub mod error;
pub mod state;
pub mod blobs;
pub mod auth;
pub mod toys;
pub mod hearts;
pub mod admin;
pub mod web;

use axum::extract::DefaultBodyLimit;
use axum::routing::get;
use axum::Router;
use state::AppState;

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(|| async { axum::Json(serde_json::json!({ "ok": true })) }))
        .merge(auth::routes())
        .merge(toys::routes())
        .merge(hearts::routes())
        .merge(admin::routes())
        // axum's default body limit is 2MB, which would reject a legitimately-capped
        // ~2MB clip once the thumb + multipart overhead push the request past it; the
        // publish handler enforces the real per-field caps (CAP_CLIP/CAP_THUMB) itself.
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024));
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
