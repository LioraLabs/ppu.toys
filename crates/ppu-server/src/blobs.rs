use crate::state::AppState;
pub fn routes() -> axum::Router<AppState> { axum::Router::new() }
