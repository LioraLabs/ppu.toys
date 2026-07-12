use std::path::PathBuf;

pub const CAP_SOURCE_PAYLOAD: usize = 128 * 1024;
pub const CAP_TOY_TOTAL: usize = 1024 * 1024;
pub const CAP_CLIP: usize = 2 * 1024 * 1024;
pub const CAP_THUMB: usize = 100 * 1024;
pub const CAP_LUA_FILE: usize = 64 * 1024;
pub const RATE_PUBLISH_PER_DAY: usize = 10;
pub const RATE_SAVE_MIN_SECS: u64 = 60;

#[derive(Clone, Debug)]
pub enum BlobMode { Db, Disk }

#[derive(Clone, Debug)]
pub struct DiscordConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub authorize_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    pub webhook_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub web_dir: PathBuf,
    pub db_path: String,
    pub blob_mode: BlobMode,
    pub data_dir: PathBuf,
    pub discord: Option<DiscordConfig>,
    pub admin_ids: Vec<String>,
    pub session_ttl_days: i64,
    pub base_url: String,
}

impl Config {
    pub fn from_env() -> Self { Self::from_map(|k| std::env::var(k).ok()) }

    pub fn from_map(get: impl Fn(&str) -> Option<String>) -> Self {
        let discord = match (get("DISCORD_CLIENT_ID"), get("DISCORD_CLIENT_SECRET"), get("DISCORD_REDIRECT_URI")) {
            (Some(client_id), Some(client_secret), Some(redirect_uri)) => Some(DiscordConfig {
                client_id, client_secret, redirect_uri,
                authorize_url: "https://discord.com/oauth2/authorize".into(),
                token_url: "https://discord.com/api/oauth2/token".into(),
                userinfo_url: "https://discord.com/api/users/@me".into(),
                webhook_url: get("DISCORD_WEBHOOK_URL"),
            }),
            _ => None,
        };
        Config {
            port: get("PPU_PORT").and_then(|s| s.parse().ok()).unwrap_or(8080),
            web_dir: get("PPU_WEB_DIR").unwrap_or_else(|| "web/dist".into()).into(),
            db_path: get("PPU_DB_PATH").unwrap_or_else(|| "ppu.db".into()),
            blob_mode: match get("PPU_BLOB_MODE").as_deref() { Some("disk") => BlobMode::Disk, _ => BlobMode::Db },
            data_dir: get("PPU_DATA_DIR").unwrap_or_else(|| "data".into()).into(),
            discord,
            admin_ids: get("PPU_ADMIN_DISCORD_IDS").map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()).unwrap_or_default(),
            session_ttl_days: get("PPU_SESSION_TTL_DAYS").and_then(|s| s.parse().ok()).unwrap_or(30),
            base_url: get("PPU_BASE_URL").unwrap_or_else(|| "http://localhost:8080".into()),
        }
    }
}
