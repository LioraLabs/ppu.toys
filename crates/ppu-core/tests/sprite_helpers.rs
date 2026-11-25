//! A helper that cuts art, called at file level with constant arguments, makes one art per call; any other
//! use of it is refused as a `sprites.art` in a function always was.
use ppu_core::sprites::{expand_helpers, plan};
use ppu_core::LuaEngine;

/// A sheet as dma() returns one: cell n is tile 2n.
const SHEET: &str = "local cells = {}\nfor i = 1, 64 do cells[i] = {tile = (i - 1) * 2} end\nlocal sheet = {tile = 0, pal = 2, cells = cells}\n";

fn planned(files: &[(&str, &str)]) -> Result<ppu_core::sprites::Plan, String> {
    let expanded = expand_helpers(files);
    let files: Vec<(&str, &str)> = match &expanded {
        Some(e) => e.iter().map(|(f, s)| (f.as_str(), s.as_str())).collect(),
        None => files.to_vec(),
    };
    plan(&files).map(|p| p.unwrap()).map_err(|e| format!("{}:{} {}", e.file, e.line, e.message))
}

#[test]
fn a_local_helper_makes_one_art_per_file_level_call_on_the_calls_line() {
    let main = format!(
        "{SHEET}local function cell(n)\n  -- one 16px cell\n  return sprites.art({{{{size = 16, columns = 1, cells = {{n}}}}}}, sheet)\nend\nlocal down = {{cell(1), cell(2)}}\nlocal still = cell(3)\nlocal walk = sprites.animation(down, 8)\nfunction frame(t, f)\n  sprites.draw(walk, 1, 1, {{at = f}})\n  sprites.draw(still, 20, 1)\nend\n"
    );
    let expanded = expand_helpers(&[("main.luau", &main)]).unwrap();
    assert_eq!(expanded[0].1.lines().count(), main.lines().count(), "{}", expanded[0].1);
    let p = planned(&[("main.luau", &main)]).unwrap();
    assert_eq!(p.arts.iter().map(|a| a.line).collect::<Vec<_>>(), [8, 8, 9]);
    assert_eq!(p.animations[0].arts, [1, 2]);
    // The preview runs it: the walk shows cells 1 and 2 (tiles 2, 4), the still cell 3 (tile 6).
    let mut engine = LuaEngine::new();
    engine.set_sources(&[("main.luau", &main)]).unwrap();
    let mut tiles = vec![];
    for f in [0, 8] {
        engine.frame(f as f64 / 60., f).unwrap();
        let mut on: Vec<u16> = engine.memory().oam.iter().filter(|o| o.on).map(|o| o.tile).collect();
        on.sort();
        tiles.push(on);
    }
    assert_eq!(tiles, [vec![2, 6], vec![4, 6]]);
}

#[test]
fn a_module_helper_makes_each_files_own_art() {
    let shared = "local M = {}\nfunction M.beetle(sheet)\n  return sprites.animation({sprites.art({{size = 32, columns = 1, cells = {0}}}, sheet), sprites.art({{size = 32, columns = 1, cells = {1}}}, sheet)}, 8)\nend\nreturn M\n";
    let level = |n: u32| {
        format!("local Shared = require(\"./shared\")\nlocal enemy_sheet = dma(\"enemy{n}\", {{char = 0x4000, pal = 0}})\nlocal BEETLE = Shared.beetle(enemy_sheet)\nlocal g = sprites.group()\nreturn {{draw = function(f) g:draw(BEETLE, 1, 1, {{at = f}}) end}}\n")
    };
    let (a, b) = (level(1), level(2));
    let main = "local A = require(\"./a\")\nlocal B = require(\"./b\")\nfunction frame(t, f)\n  A.draw(f)\n  B.draw(f)\nend\n";
    let p = planned(&[("shared.luau", shared), ("a.luau", &a), ("b.luau", &b), ("main.luau", main)]).unwrap();
    let sites: Vec<(&str, u32)> = p.arts.iter().map(|s| (s.file.as_str(), s.line)).collect();
    assert_eq!(sites, [("a.luau", 3), ("a.luau", 3), ("b.luau", 3), ("b.luau", 3)]);
    assert_eq!(p.animations.iter().map(|a| a.arts.clone()).collect::<Vec<_>>(), [vec![1, 2], vec![3, 4]]);
}

#[test]
fn an_exported_local_helper_takes_art_its_caller_made() {
    // Platformer's shape: the middle step is one art the animation shows twice, made by the level and passed in.
    let level = "local function beetle(sheet: any, mid: any)\n  return sprites.animation({sprites.art({{size = 32, columns = 1, cells = {0}}}, sheet), mid, sprites.art({{size = 32, columns = 1, cells = {2}}}, sheet), mid}, 8)\nend\nlocal function read() return 1 end\nreturn {read = read, beetle = beetle}\n";
    let meadow = "local Level = require(\"./level\")\nlocal enemy_sheet = dma(\"enemies\", {char = 0x4000, pal = 0})\nlocal beetle_mid = sprites.art({{size = 32, columns = 1, cells = {1}}}, enemy_sheet)\nlocal BEETLE = Level.beetle(enemy_sheet, beetle_mid)\nlocal g = sprites.group()\nfunction frame(t, f) g:draw(BEETLE, 1, 1, {at = f}) end\n";
    let files = [("level.luau", level), ("main.luau", meadow)];
    let expanded = expand_helpers(&files).unwrap();
    assert!(expanded[0].1.contains("return {read = read, beetle = nil}"), "{}", expanded[0].1);
    let p = planned(&files).unwrap();
    assert_eq!(p.arts.iter().map(|a| a.line).collect::<Vec<_>>(), [3, 4, 4]);
    assert_eq!(p.animations[0].arts, [2, 1, 3, 1]);
}

#[test]
fn a_helper_called_in_a_function_a_loop_or_with_a_computed_argument_is_refused() {
    let helper = format!("{SHEET}local function cell(n)\n  return sprites.art({{{{size = 16, columns = 1, cells = {{n}}}}}}, sheet)\nend\n");
    let refused = "main.luau:5 sprites.art must be called at file level, outside functions and loops, so each call makes one art (a helper returning it is fine when every call of the helper is at file level with constant arguments)";
    for tail in [
        "local a = cell(1)\nfunction frame(t, f) sprites.draw(cell(2), 1, 1) end\n",
        "local a = cell(1)\nfor i = 1, 2 do local b = cell(i) end\nfunction frame(t, f) sprites.draw(a, 1, 1) end\n",
        "local a = cell(1 + 1)\nfunction frame(t, f) sprites.draw(a, 1, 1) end\n",
        "local pick = cell\nfunction frame(t, f) sprites.draw(pick(1), 1, 1) end\n",
    ] {
        assert_eq!(planned(&[("main.luau", &format!("{helper}{tail}"))]).unwrap_err(), refused, "{tail}");
    }
}

#[test]
fn a_helper_reading_its_own_files_local_is_refused_from_another_file() {
    // `sheet` means shared.luau's local there, and nothing at main's call.
    let shared = format!("{SHEET}local M = {{}}\nfunction M.cell(n)\n  return sprites.art({{{{size = 16, columns = 1, cells = {{n}}}}}}, sheet)\nend\nreturn M\n");
    let main = "local S = require(\"./shared\")\nlocal a = S.cell(1)\nfunction frame(t, f) sprites.draw(a, 1, 1) end\n";
    let e = planned(&[("shared.luau", &shared), ("main.luau", main)]).unwrap_err();
    assert!(e.starts_with("shared.luau:6 sprites.art must be called at file level"), "{e}");
}

#[test]
fn a_helper_that_makes_a_group_makes_one_per_file_level_call() {
    let main = format!("{SHEET}local a = sprites.art({{{{size = 16, columns = 1, cells = {{1}}}}}}, sheet)\nlocal function layer(n)\n  return sprites.group({{slots = n}})\nend\nlocal back, front = layer(2), layer(3)\nfunction frame(t, f)\n  back:draw(a, 1, 1)\n  front:draw(a, 2, 2)\nend\n");
    let negative = main.replace("layer(3)", "layer(-3)");
    let expanded = expand_helpers(&[("main.luau", &negative)]).unwrap();
    assert!(expanded[0].1.contains("sprites.group({slots = (-3)})"), "{}", expanded[0].1);
    let p = planned(&[("main.luau", &main)]).unwrap();
    assert_eq!(p.groups.iter().filter_map(|g| g.site.as_ref().map(|s| s.line)).collect::<Vec<_>>(), [8, 8]);
}

#[test]
fn a_program_without_art_helpers_is_left_as_written() {
    let main = format!("{SHEET}local a = sprites.art({{{{size = 16, columns = 1, cells = {{1}}}}}}, sheet)\nlocal function f(x) return x + 1 end\nlocal b = f(2)\n");
    assert!(expand_helpers(&[("main.luau", &main)]).is_none());
}
