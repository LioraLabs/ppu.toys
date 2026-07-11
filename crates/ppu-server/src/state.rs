use crate::config::Config;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub pool: SqlitePool,
    pub http: reqwest::Client,
    pub limiter: RateLimiter,
}

#[derive(Clone, Default)]
pub struct RateLimiter;
impl RateLimiter {
    pub fn check_save(&self, _user: &str) -> bool { true }
    pub fn check_publish(&self, _user: &str) -> bool { true }
}

impl AppState {
    pub fn new(cfg: Config, pool: SqlitePool) -> Self {
        AppState { cfg: Arc::new(cfg), pool, http: reqwest::Client::new(), limiter: RateLimiter::default() }
    }
}
