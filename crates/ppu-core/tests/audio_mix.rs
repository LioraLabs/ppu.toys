use ppu_core::{AudioMix, LuaEngine, AUDIO_MIX_FILE};

const SONG: &str = r#"
local ticks = 0
timer(0, 32, function()
  ticks = ticks + 1
  voice[0].pitch = 1000 + ticks
  voice[0].vol = {l=-100,r=100}
  voice[1].vol = {l=80,r=80}
end)
function frame() brightness = ticks end
"#;

#[test]
fn live_mix_follows_timer_writes_without_compounding_or_restarting() {
    let mut e = LuaEngine::new();
    e.set_source(SONG).unwrap();
    let mut mix = AudioMix::default();
    mix.voices[0].left_db = -6.0;
    mix.voices[0].transpose = 12.0;
    mix.voices[0].solo = true;
    let json = serde_json::to_string(&mix).unwrap();
    e.set_audio_mix(Some(&json)).unwrap();
    e.frame(0.0, 0).unwrap();
    let first = e.dsp_view();
    assert_eq!(first.voices[0].vol.l, -50);
    assert_eq!(first.voices[1].vol.l, 0);
    assert_eq!(first.voices[0].pitch, 2008); // four timer expiries, then transpose
    e.set_audio_mix(None).unwrap();
    assert_eq!(e.frame(0.02, 1).unwrap().rows[0].brightness, 4);
    assert_eq!(e.dsp_view().voices[0].vol.l, -100);
    assert_eq!(e.dsp_view().voices[1].vol.l, 80);
    assert_eq!(e.dsp_view().voices[0].pitch, 1008);
}

#[test]
fn saved_mix_only_update_keeps_timer_phase_and_runs_in_a_fresh_engine() {
    let mut e = LuaEngine::new();
    e.set_sources(&[("main.lua", SONG)]).unwrap();
    e.frame(0.0, 0).unwrap();
    let json = r#"{"masterDb":-6}"#;
    e.set_sources(&[("main.lua", SONG), (AUDIO_MIX_FILE, json)])
        .unwrap();
    assert_eq!(e.frame(0.02, 1).unwrap().rows[0].brightness, 4);
    assert_eq!(e.audio_mix().master_db, -6.0);
    let mut fresh = LuaEngine::new();
    fresh
        .set_sources(&[("main.lua", SONG), (AUDIO_MIX_FILE, json)])
        .unwrap();
    assert_eq!(fresh.audio_mix().master_db, -6.0);
    fresh.reset().unwrap();
    assert_eq!(fresh.audio_mix().master_db, -6.0);
}

#[test]
fn rejects_bad_mix_and_echo_overlap_without_replacing_current_mix() {
    assert!(AudioMix::parse(r#"{"voices":[]}"#).is_err());
    assert!(AudioMix::parse(r#"{"echoDelay":16}"#).is_err());
    assert!(AudioMix::parse(r#"{"masterDb":100}"#).is_err());
    let mut mix = AudioMix::default();
    mix.echo_delay = Some(15);
    assert!(mix
        .validate_samples(&[ppu_core::DspSampleView {
            id: 0,
            name: "bass".into(),
            start: 0x9000,
            end: 0xa000
        }])
        .is_err());
    let mut e = LuaEngine::new();
    e.set_audio_mix(Some(r#"{"masterDb":-3}"#)).unwrap();
    assert!(e.set_audio_mix(Some(r#"{"masterDb":99}"#)).is_err());
    assert_eq!(e.audio_mix().master_db, -3.0);
    assert!(e.trigger_voice(8, false).is_err());
}

#[test]
fn hot_recompile_does_not_bake_trims_into_song_registers() {
    let mut e = LuaEngine::new();
    e.set_source("voice[0].vol.l=100\nfunction frame() end")
        .unwrap();
    let mut mix = AudioMix::default();
    mix.voices[0].left_db = -6.0;
    e.set_audio_mix(Some(&serde_json::to_string(&mix).unwrap()))
        .unwrap();
    e.frame(0.0, 0).unwrap();
    e.set_source("function frame() end").unwrap();
    e.frame(0.0, 1).unwrap();
    assert_eq!(e.dsp_view().voices[0].vol.l, 50);
    e.set_audio_mix(None).unwrap();
    e.frame(0.0, 2).unwrap();
    assert_eq!(e.dsp_view().voices[0].vol.l, 100);
}

#[test]
fn live_trigger_and_release_are_one_shots_and_mute_keeps_the_envelope_running() {
    let mut e = LuaEngine::new();
    e.set_source(
        r#"
aram[0x100]=0 aram[0x101]=5 aram[0x102]=0 aram[0x103]=5
aram[0x500]=3
voice[0].pitch=4096 voice[0].gain=127 voice[0].noise=true
voice[0].vol={l=127,r=127} dsp.noise_clock=31 dsp.mvol={l=127,r=127}
function frame() end
"#,
    )
    .unwrap();
    e.trigger_voice(0, false).unwrap();
    e.frame(0.0, 0).unwrap();
    assert_eq!(e.dsp_view().voices[0].envx, 127);
    assert!(e.audio().iter().any(|&v| v != 0));
    let mut mix = AudioMix::default();
    mix.voices[0].mute = true;
    e.set_audio_mix(Some(&serde_json::to_string(&mix).unwrap()))
        .unwrap();
    e.frame(0.02, 1).unwrap();
    assert_eq!(e.dsp_view().voices[0].envx, 127);
    e.trigger_voice(0, true).unwrap();
    e.frame(0.04, 2).unwrap();
    assert_eq!(e.dsp_view().voices[0].envx, 0);
    e.frame(0.06, 3).unwrap();
    assert_eq!(e.dsp_view().voices[0].envx, 0);
}
