use ppu_server::config::{BlobMode, Config};

#[test]
fn defaults_apply_and_missing_discord_is_none() {
    let cfg = Config::from_map(|_| None);
    assert_eq!(cfg.port, 8080);
    assert_eq!(cfg.web_dir.to_str().unwrap(), "web/dist");
    assert!(matches!(cfg.blob_mode, BlobMode::Db));
    assert_eq!(cfg.session_ttl_days, 30);
    assert!(cfg.discord.is_none(), "no creds => auth disabled");
    assert!(cfg.admin_ids.is_empty());
}

#[test]
fn discord_present_and_admin_ids_parsed() {
    let vars = |k: &str| match k {
        "DISCORD_CLIENT_ID" => Some("cid".to_string()),
        "DISCORD_CLIENT_SECRET" => Some("secret".to_string()),
        "DISCORD_REDIRECT_URI" => Some("http://localhost/api/auth/callback".to_string()),
        "PPU_ADMIN_DISCORD_IDS" => Some("111, 222".to_string()),
        "PPU_BLOB_MODE" => Some("disk".to_string()),
        _ => None,
    };
    let cfg = Config::from_map(vars);
    let d = cfg.discord.expect("discord configured");
    assert_eq!(d.client_id, "cid");
    assert!(matches!(cfg.blob_mode, BlobMode::Disk));
    assert_eq!(cfg.admin_ids, vec!["111".to_string(), "222".to_string()]);
}
