use crate::config::Config;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub pool: SqlitePool,
    pub http: reqwest::Client,
    pub limiter: RateLimiter,
}

#[derive(Clone, Default)]
pub struct RateLimiter { inner: Arc<Mutex<HashMap<String, UserLimit>>> }

#[derive(Default)]
struct UserLimit { last_save: Option<Instant>, publishes: Vec<Instant> }

impl RateLimiter {
    pub fn check_save(&self, user: &str) -> bool {
        let mut g = self.inner.lock().unwrap();
        let e = g.entry(user.to_string()).or_default();
        let now = Instant::now();
        if let Some(t) = e.last_save { if now.duration_since(t).as_secs() < crate::config::RATE_SAVE_MIN_SECS { return false; } }
        e.last_save = Some(now); true
    }
    pub fn check_publish(&self, user: &str) -> bool {
        let mut g = self.inner.lock().unwrap();
        let e = g.entry(user.to_string()).or_default();
        let now = Instant::now();
        e.publishes.retain(|t| now.duration_since(*t).as_secs() < 86400);
        if e.publishes.len() >= crate::config::RATE_PUBLISH_PER_DAY { return false; }
        e.publishes.push(now); true
    }
}

impl AppState {
    pub fn new(cfg: Config, pool: SqlitePool) -> Self {
        AppState { cfg: Arc::new(cfg), pool, http: reqwest::Client::new(), limiter: RateLimiter::default() }
    }
}
