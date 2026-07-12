use ppu_server::state::RateLimiter;

#[test]
fn save_limited_to_one_per_minute_publish_capped_per_day() {
    let rl = RateLimiter::default();
    assert!(rl.check_save("u"));
    assert!(!rl.check_save("u"), "second save within a minute blocked");
    assert!(rl.check_save("other"), "per-user, not global");
    for _ in 0..10 { assert!(rl.check_publish("p")); }
    assert!(!rl.check_publish("p"), "11th publish in a day blocked");
}
