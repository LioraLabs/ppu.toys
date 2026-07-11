use crate::state::AppState;
pub fn routes(_state: &AppState) -> axum::Router<AppState> { axum::Router::new() }
