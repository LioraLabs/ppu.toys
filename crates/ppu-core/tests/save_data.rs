//! `saveData`: the preview keeps the record as the cartridge's SRAM bytes (ppu_core::save), in the
//! host's save slot as `sram["ppu.save"]` hex, so a reload keeps it and a `.srm` moves both ways.
use ppu_core::save::{fresh, layout, Kind};
use ppu_core::LuaEngine;
use serde_json::Value;

const MAIN: &str = "local save = saveData({stage = makeBounded(1, 1, 8), hiscore = makeBounded(0, 0, 999999), seen_intro = false})\n\
local score = 0\n\
function frame(t, f)\n\
  if pad.a then score += 1000 end\n\
  if score > save.hiscore then save.hiscore = score end\n\
  if f == 3 then save.seen_intro = true; save.stage += 2 end\n\
  brightness = save.stage\n\
end\n";

fn hex(bytes: &[u8]) -> String {
    let end = bytes.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    bytes[..end].iter().map(|b| format!("{b:02x}")).collect()
}

fn slot(e: &mut LuaEngine) -> Option<String> {
    let json: Value = serde_json::from_str(&e.take_sram()?).unwrap();
    json["ppu.save"].as_str().map(String::from)
}

fn the_layout() -> ppu_core::save::Layout {
    layout(&[
        ("stage".into(), Kind::Bounded { lo: 1, hi: 8 }),
        ("hiscore".into(), Kind::Bounded { lo: 0, hi: 999_999 }),
        ("seen_intro".into(), Kind::Boolean),
    ])
    .unwrap()
}

#[test]
fn first_boot_writes_the_defaults_then_writes_update_words_and_checksum() {
    let l = the_layout();
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.luau", MAIN)]).unwrap();
    e.frame(0.0, 0).unwrap();
    // hiscore 0, seen_intro false, stage 1 (stored 0).
    assert_eq!(slot(&mut e).unwrap(), hex(&fresh(&l, &[0, 0, 0])));
    e.set_pad(1 << 4);
    for f in 1..5 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    // Four presses: 4000; frame 3 set seen_intro and stage 3.
    assert_eq!(slot(&mut e).unwrap(), hex(&fresh(&l, &[4000, 1, 2])));
    // A score past 65535 takes the high word.
    for f in 5..70 {
        e.frame(f as f64 / 60.0, f).unwrap();
    }
    let saved = slot(&mut e).unwrap();
    assert_eq!(saved, hex(&fresh(&l, &[69_000, 1, 2])));

    // A reload (fresh engine on the saved slot) reads the record back.
    let mut e2 = LuaEngine::new();
    e2.set_sram(&format!(r#"{{"ppu.save":"{saved}","ppu.save.size":2048}}"#));
    e2.set_sources(&[("main.luau", MAIN)]).unwrap();
    e2.set_pad(0);
    assert_eq!(e2.frame(0.0, 0).unwrap().rows[0].brightness, 3);
    assert_eq!(e2.take_sram(), None, "a kept save is not rewritten");
}

#[test]
fn a_damaged_or_foreign_save_starts_from_the_defaults() {
    let l = the_layout();
    let mut good = fresh(&l, &[500, 1, 6]);
    for (damage, why) in [(4usize, "checksum"), (0, "magic"), (2, "layout id")] {
        good[damage] ^= 0x10;
        let mut e = LuaEngine::new();
        e.set_sram(&format!(r#"{{"ppu.save":"{}"}}"#, hex(&good)));
        e.set_sources(&[("main.luau", MAIN)]).unwrap();
        assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 1, "{why}");
        assert_eq!(slot(&mut e).unwrap(), hex(&fresh(&l, &[0, 0, 0])), "{why}");
        good[damage] ^= 0x10;
    }
    // Intact but out of range (stage stored 9): defaults too.
    let mut e = LuaEngine::new();
    e.set_sram(&format!(r#"{{"ppu.save":"{}"}}"#, hex(&fresh(&l, &[0, 0, 9]))));
    e.set_sources(&[("main.luau", MAIN)]).unwrap();
    assert_eq!(e.frame(0.0, 0).unwrap().rows[0].brightness, 1);
}

#[test]
fn fields_keep_their_contracts_and_the_declaration_is_checked() {
    let run = |main: &str| {
        let mut e = LuaEngine::new();
        e.set_sources(&[("main.luau", main)]).and_then(|_| e.frame(0.0, 0).map(|_| ()))
    };
    let e = run("local save = saveData({lives = makeBounded(3, 0, 9)})\nfunction frame() save.lives = 10 end").unwrap_err();
    assert!(e.message.contains("0..9"), "{}", e.message);
    let e = run("local save = saveData({seen = false})\nfunction frame() save.other = 1 end").unwrap_err();
    assert!(e.message.contains("no field other"), "{}", e.message);
    let e = run("local MAX = 9\nlocal save = saveData({lives = makeBounded(3, 0, MAX)})\nfunction frame() end").unwrap_err();
    assert!(e.message.contains("literal bounds"), "{}", e.message);
    let e = run("local save = saveData({speed = makeFixed(0, 0, 4)})\nfunction frame() end").unwrap_err();
    assert!(e.message.contains("saveData field speed"), "{}", e.message);
}

#[test]
fn a_foreign_save_full_of_junk_resets_quickly() {
    // Another game's 2 KiB: every word changes when the record starts over.
    let junk: Vec<u8> = (0..2048).map(|i| ((i * 37 + 11) & 0xff) as u8).collect();
    let mut e = LuaEngine::new();
    e.set_sram(&format!("{{{}}}", ppu_core::save::slot_members(&junk)));
    let start = std::time::Instant::now();
    e.set_sources(&[("main.luau", MAIN)]).unwrap();
    e.frame(0.0, 0).unwrap();
    assert!(start.elapsed().as_millis() < 500, "{:?}", start.elapsed());
    assert_eq!(slot(&mut e).unwrap(), hex(&fresh(&the_layout(), &[0, 0, 0])));
}
