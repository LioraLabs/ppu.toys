//! `ppu check`'s cartridge half: the check module the site's Studio runs (the SNES compiler's
//! checks, no ROM path), fetched from the site, cached by its content hash and run here through
//! wasmtime over the same toy and frames as the preview check. The module is the site's, under its
//! own licence: ppu downloads it and never ships it.

use crate::login;
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use wasmtime::{
    Caller, Config, Engine, Extern, ExternRef, Func, Instance, Module, Ref, Store, Val,
};

/// The longest run the module takes (inspect's own limit).
const MAX_SECONDS: f64 = 600.0;

/// Where downloaded check modules live: `check-module/` in `PPU_CACHE_DIR`, or in the platform's
/// user cache directory (`%LOCALAPPDATA%`, `~/Library/Caches`, `$XDG_CACHE_HOME` or `~/.cache`).
fn cache_dir() -> Result<PathBuf> {
    let var = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let dir = if let Some(d) = var("PPU_CACHE_DIR") {
        d
    } else if cfg!(windows) {
        var("LOCALAPPDATA")
            .context("LOCALAPPDATA is not set")?
            .join("ppu")
    } else if cfg!(target_os = "macos") {
        var("HOME")
            .context("HOME is not set")?
            .join("Library/Caches/ppu")
    } else if let Some(x) = var("XDG_CACHE_HOME") {
        x.join("ppu")
    } else {
        var("HOME").context("HOME is not set")?.join(".cache/ppu")
    };
    Ok(dir.join("check-module"))
}

/// A content hash as the site names it (Vite's base64url hash): safe as a file name.
fn valid_hash(hash: &str) -> bool {
    !hash.is_empty()
        && hash.len() <= 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// A download that is not the module the site named: never cached and never run, and no reason
/// to fall back on the cached one as if offline.
#[derive(Debug)]
struct Refused(String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Ask the site which module it serves and download it unless the cache has it; returns its hash.
fn fetch(dir: &Path, out: &mut dyn Write) -> Result<String> {
    let creds = login::load(&login::credentials_path()?)?;
    let site = creds
        .as_ref()
        .map_or_else(login::site_from_env, |c| c.site.clone());
    let http = login::agent(Duration::from_secs(60));
    let req = match &creds {
        Some(c) => login::request_on(&http, c, "GET", "/api/check-module"),
        None => http.get(&format!("{site}/api/check-module")),
    };
    let (status, v) = login::call(req, None)?;
    ensure!(
        status == 200,
        "{site} has no check module: {}",
        login::refusal(status, &v)
    );
    let hash = v["hash"].as_str().unwrap_or_default();
    let url = v["url"].as_str().unwrap_or_default();
    ensure!(
        valid_hash(hash) && url.starts_with('/'),
        "{site} named its check module oddly: {v}"
    );
    let wasm = dir.join(format!("{hash}.wasm"));
    if !wasm.exists() {
        writeln!(out, "Fetching the check module {hash} from {site}…")?;
        out.flush()?;
        let res = http
            .get(&format!("{site}{url}"))
            .call()
            .with_context(|| format!("cannot download {site}{url}"))?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut res.into_reader(), &mut bytes)?;
        let want = v["sha256"].as_str().unwrap_or_default();
        let got = sha256_hex(&bytes);
        if want != got {
            return Err(Refused(format!(
                "the check module {site}{url} does not match the sha256 {site} served for it \
                 (served {want:?}, downloaded {got}); not cached, not run"
            ))
            .into());
        }
        std::fs::create_dir_all(dir)?;
        // write then rename: an interrupted download never looks cached
        let part = dir.join(format!("{hash}.wasm.part"));
        std::fs::write(&part, &bytes)?;
        std::fs::rename(&part, &wasm)?;
        if let Some(licence) = v["licence"].as_str() {
            std::fs::write(dir.join(format!("{hash}.LICENSE")), licence)?;
        }
    }
    std::fs::write(dir.join("latest"), hash)?;
    Ok(hash.to_string())
}

/// The module to run: the site's, or offline the last one fetched, saying so. None, said as a
/// NOT CHECKED line beside the preview's PASS: offline before any module was fetched, so only the
/// preview check runs. Err: the download was not the module the site named.
fn module_hash(dir: &Path, out: &mut dyn Write) -> Result<Option<String>> {
    let e = match fetch(dir, out) {
        Ok(hash) => return Ok(Some(hash)),
        Err(e) if e.is::<Refused>() => return Err(e),
        Err(e) => e,
    };
    let cached = std::fs::read_to_string(dir.join("latest"))
        .ok()
        .map(|h| h.trim().to_string())
        .filter(|h| valid_hash(h) && dir.join(format!("{h}.wasm")).exists());
    match cached {
        Some(hash) => {
            writeln!(out, "Offline ({e}): using the cached check module {hash}.")?;
            Ok(Some(hash))
        }
        None => {
            writeln!(out, "NOT CHECKED: the cartridge check did not run: {e}, and no check module is cached yet; run ppu check once online")?;
            Ok(None)
        }
    }
}

/// `PPU_CHECK_MODULE`'s file, cached under its content hash like a downloaded one.
fn local_module(dir: &Path, path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("PPU_CHECK_MODULE: cannot read {}", path.display()))?;
    let hash = format!("local-{}", &sha256_hex(&bytes)[..16]);
    let wasm = dir.join(format!("{hash}.wasm"));
    if !wasm.exists() {
        std::fs::create_dir_all(dir)?;
        let part = dir.join(format!("{hash}.wasm.part"));
        std::fs::write(&part, &bytes)?;
        std::fs::rename(&part, &wasm)?;
    }
    Ok(hash)
}

/// `bytes` followed by their checksum.
// shortcut: SipHash with fixed keys catches a damaged file, not a forged one; anyone who can write
// the cache can replace the .wasm too. Upgrade to a keyed MAC if the cache ever crosses users.
fn seal(mut bytes: Vec<u8>) -> Vec<u8> {
    let sum = checksum(&bytes);
    bytes.extend_from_slice(&sum);
    bytes
}

/// What `seal` wrapped, or None when the checksum does not match (a damaged or older file).
fn unseal(sealed: &[u8]) -> Option<&[u8]> {
    let (bytes, sum) = sealed.split_at_checked(sealed.len().checked_sub(8)?)?;
    (checksum(bytes) == sum).then_some(bytes)
}

fn checksum(bytes: &[u8]) -> [u8; 8] {
    use std::hash::Hasher;
    #[allow(deprecated)] // SipHasher's keys are fixed: the sum is the same in every run
    let mut h = std::hash::SipHasher::new();
    h.write(bytes);
    h.finish().to_le_bytes()
}

/// The compiled module: wasmtime's own serialized form, sealed and cached beside the .wasm,
/// compiled again when it is missing, damaged, or another wasmtime wrote it.
fn load(engine: &Engine, dir: &Path, hash: &str) -> Result<Module> {
    let compiled = dir.join(format!("{hash}.cwasm"));
    let cached = std::fs::read(&compiled).ok();
    // Safety: the bytes are what this function serialized (the seal matches); deserialize refuses
    // what another wasmtime or engine config wrote.
    if let Some(m) = cached
        .as_deref()
        .and_then(unseal)
        .and_then(|b| unsafe { Module::deserialize(engine, b) }.ok())
    {
        return Ok(m);
    }
    let wasm = dir.join(format!("{hash}.wasm"));
    let module = Module::from_file(engine, &wasm)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("cannot compile the check module {}", wasm.display()))?;
    if let Ok(bytes) = module.serialize() {
        let part = dir.join(format!("{hash}.cwasm.part"));
        if std::fs::write(&part, seal(bytes)).is_ok() {
            let _ = std::fs::rename(&part, &compiled);
        }
    }
    Ok(module)
}

/// The JavaScript values the module's imports pass around: the global object, its `crypto`, and
/// byte arrays (getrandom's buffer, a slice or string the module hands over).
enum Js {
    Global,
    Crypto,
    Bytes(Vec<u8>),
}

/// An import's name without wasm-bindgen's `__wbg_` prefix and per-build hash suffix.
fn base_name(name: &str) -> &str {
    let name = name.strip_prefix("__wbg_").unwrap_or(name);
    match name.rsplit_once('_') {
        Some((base, hash))
            if hash.len() == 16
                && hash.bytes().all(|b| b.is_ascii_hexdigit())
                && !base.ends_with("cast") =>
        {
            base
        }
        _ => name,
    }
}

fn js<'a>(caller: &'a Caller<'_, u64>, v: &Val) -> Option<&'a Js> {
    let r = v.unwrap_externref()?;
    r.data(caller).ok()??.downcast_ref::<Js>()
}

fn bytes_mut<'a>(caller: &'a mut Caller<'_, u64>, v: &Val) -> Result<&'a mut Vec<u8>> {
    let r = *v
        .unwrap_externref()
        .context("the check module passed undefined for an array")?;
    match r.data_mut(caller)?.and_then(|d| d.downcast_mut::<Js>()) {
        Some(Js::Bytes(b)) => Ok(b),
        _ => bail!("the check module passed a non-array where an array goes"),
    }
}

fn memory(caller: &mut Caller<'_, u64>) -> Result<wasmtime::Memory> {
    match caller.get_export("memory") {
        Some(Extern::Memory(m)) => Ok(m),
        _ => bail!("the check module has no memory"),
    }
}

fn slice(caller: &mut Caller<'_, u64>, ptr: &Val, len: &Val) -> Result<Vec<u8>> {
    let (ptr, len) = (
        ptr.unwrap_i32() as u32 as usize,
        len.unwrap_i32() as u32 as usize,
    );
    let m = memory(caller)?;
    Ok(m.data(&caller)
        .get(ptr..ptr + len)
        .context("the check module passed bytes outside its memory")?
        .to_vec())
}

fn value(caller: &mut Caller<'_, u64>, v: Js) -> Result<Val> {
    Ok(Val::ExternRef(Some(ExternRef::new(caller, v)?)))
}

/// The host side of the module's wasm-bindgen imports: just enough of a JavaScript global for the
/// check to run outside a browser. Its one use of the outside world is getrandom's
/// `crypto.getRandomValues` (the Lua VM seeds its hashing), answered with a fixed sequence so a
/// check is a function of the toy alone; any other import stops the check naming it.
fn host(name: &str, mut caller: Caller<'_, u64>, p: &[Val], results: &mut [Val]) -> Result<()> {
    let flag = |b: bool| Val::I32(i32::from(b));
    match base_name(name) {
        // JavaScript seeds the table with undefined/null/true/false for its own lookups; the host
        // reads no table slot, and this runs during instantiation, before exports are reachable.
        "__wbindgen_init_externref_table" => {}
        "__wbindgen_throw" => {
            let text = slice(&mut caller, &p[0], &p[1])?;
            bail!("the check module threw: {}", String::from_utf8_lossy(&text));
        }
        n if n.starts_with("static_accessor_") => {
            let Some(Extern::Func(alloc)) = caller.get_export("__externref_table_alloc") else {
                bail!("the check module has no externref table");
            };
            let Some(Extern::Table(table)) = caller.get_export("__wbindgen_externrefs") else {
                bail!("the check module has no externref table");
            };
            let mut at = [Val::I32(0)];
            alloc.call(&mut caller, &[], &mut at)?;
            let global = ExternRef::new(&mut caller, Js::Global)?;
            table.set(
                &mut caller,
                at[0].unwrap_i32() as u32 as u64,
                Ref::Extern(Some(global)),
            )?;
            results[0] = at[0];
        }
        "crypto" => results[0] = value(&mut caller, Js::Crypto)?,
        "__wbindgen_is_object" => {
            results[0] = flag(matches!(js(&caller, &p[0]), Some(Js::Global | Js::Crypto)))
        }
        "__wbindgen_is_undefined" => results[0] = flag(p[0].unwrap_externref().is_none()),
        "__wbindgen_is_function" | "__wbindgen_is_string" => results[0] = flag(false),
        "new_with_length" => {
            results[0] = value(
                &mut caller,
                Js::Bytes(vec![0; p[0].unwrap_i32() as u32 as usize]),
            )?
        }
        "length" => {
            results[0] = Val::I32(bytes_mut(&mut caller, &p[0])?.len() as i32);
        }
        // a copy stands in for the view: getrandom fills the subarray, then copies it out
        "subarray" => {
            let (from, to) = (
                p[1].unwrap_i32() as u32 as usize,
                p[2].unwrap_i32() as u32 as usize,
            );
            let part = bytes_mut(&mut caller, &p[0])?
                .get(from..to)
                .context("subarray out of range")?
                .to_vec();
            results[0] = value(&mut caller, Js::Bytes(part))?;
        }
        "getRandomValues" => {
            let mut state = *caller.data();
            for b in bytes_mut(&mut caller, &p[1])?.iter_mut() {
                // splitmix64
                state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
                let mut z = state;
                z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
                *b = (z ^ (z >> 31)) as u8;
            }
            *caller.data_mut() = state;
        }
        // Uint8Array.prototype.set.call(memory[ptr..ptr+len], array): the array into memory
        "prototypesetcall" => {
            let (ptr, len) = (
                p[0].unwrap_i32() as u32 as usize,
                p[1].unwrap_i32() as u32 as usize,
            );
            let data = bytes_mut(&mut caller, &p[2])?.clone();
            ensure!(
                data.len() <= len,
                "the check module copied an array into too small a slice"
            );
            memory(&mut caller)?.write(&mut caller, ptr, &data)?;
        }
        n if n.starts_with("__wbindgen_cast_") => {
            let bytes = slice(&mut caller, &p[0], &p[1])?;
            results[0] = value(&mut caller, Js::Bytes(bytes))?;
        }
        _ => bail!("the check module asked the host for {name}, which ppu check does not provide"),
    }
    Ok(())
}

/// `inspect(body)` in the module: JSON in, JSON out.
fn inspect(engine: &Engine, module: &Module, body: &str) -> Result<String> {
    let mut store = Store::new(engine, 0u64);
    let imports = module
        .imports()
        .map(|i| {
            let ty = i.ty().func().cloned().with_context(|| {
                format!("the check module imports {}, not a function", i.name())
            })?;
            let name = i.name().to_string();
            Ok(Func::new(&mut store, ty, move |c, p, r| {
                host(&name, c, p, r).map_err(wasmtime::Error::from_anyhow)
            })
            .into())
        })
        .collect::<Result<Vec<Extern>>>()?;
    let instance = Instance::new(&mut store, module, &imports)?;
    if let Ok(start) = instance.get_typed_func::<(), ()>(&mut store, "__wbindgen_start") {
        start.call(&mut store, ())?;
    }
    let memory = instance
        .get_memory(&mut store, "memory")
        .context("the check module has no memory")?;
    let malloc = instance.get_typed_func::<(u32, u32), u32>(&mut store, "__wbindgen_malloc")?;
    let run = instance.get_typed_func::<(u32, u32), (u32, u32)>(&mut store, "inspect")?;
    let len = u32::try_from(body.len()).context("the toy is too big for the check module")?;
    let ptr = malloc.call(&mut store, (len, 1))?;
    memory.write(&mut store, ptr as usize, body.as_bytes())?;
    let (at, n) = run.call(&mut store, (ptr, len))?;
    let text = memory
        .data(&store)
        .get(at as usize..at as usize + n as usize)
        .context("the check module answered outside its memory")?;
    Ok(String::from_utf8(text.to_vec())?)
}

/// `holds` (`(mask, from, to)` spans, pad 2 in the high half) as inspect's plan: at every span
/// edge, the buttons of the spans covering it.
fn plan(holds: &[(u32, u32, u32)]) -> Value {
    let names = |mask: u32| -> Vec<&str> {
        ppu_core::PAD_NAMES
            .iter()
            .enumerate()
            .filter(|(bit, _)| mask & 1 << bit != 0)
            .map(|(_, n)| *n)
            .collect()
    };
    let edges = holds
        .iter()
        .flat_map(|(_, from, to)| [*from, *to])
        .map(|f| {
            let mask = holds
                .iter()
                .filter(|(_, from, to)| (*from..*to).contains(&f))
                .fold(0, |m, (b, _, _)| m | b);
            json!({"frame": f, "buttons": names(mask & 0xffff), "buttons2": names(mask >> 16)})
        });
    Value::Array(edges.collect())
}

/// Run the cartridge check over `path` (a folder or a packed toy) for frames `0..=last` under
/// `holds`, printing each finding to `out` as `file:line: severity[code]: message`, its fixes, then
/// the docs pointer to its note. Fails when the cartridge would refuse or stop.
pub fn check(path: &Path, last: u32, holds: &[(u32, u32, u32)], out: &mut dyn Write) -> Result<()> {
    let text = if path.is_dir() {
        crate::pack(path)?
    } else {
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?
    };
    let mut body: Value = serde_json::from_str(&text).context("invalid packed toy")?;
    body["seconds"] = (f64::from(last + 1) / 60.0).min(MAX_SECONDS).into();
    body["plan"] = plan(holds);
    let body = body.to_string();

    let dir = cache_dir()?;
    let hash = match std::env::var_os("PPU_CHECK_MODULE").filter(|v| !v.is_empty()) {
        // A module built beside the code it checks (the repo's own gate), not the site's.
        Some(path) => local_module(&dir, Path::new(&path))?,
        None => match module_hash(&dir, out)? {
            Some(hash) => hash,
            None => return Ok(()),
        },
    };
    let mut config = Config::new();
    // a big game's Lua recurses deeply; the run thread below has room for it
    config.max_wasm_stack(32 << 20);
    config.async_stack_size(33 << 20);
    let engine = Engine::new(&config)?;
    let module = load(&engine, &dir, &hash)?;
    let answer = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(move || inspect(&engine, &module, &body))?
        .join()
        .map_err(|_| anyhow::anyhow!("the check module's thread panicked"))?
        .map_err(|e| anyhow::anyhow!("the check module {hash} crashed (a compiler bug): {e:#}"))?;
    let v: Value = serde_json::from_str(&answer).context("the check module answered oddly")?;
    report(path, &v, out)
}

/// Print an inspection; Err when it refuses the toy or its run stops.
fn report(path: &Path, v: &Value, out: &mut dyn Write) -> Result<()> {
    if let Some(e) = v["error"].as_str() {
        bail!("the cartridge check could not read the toy: {e}");
    }
    let base = if path.is_dir() { path } else { Path::new(".") };
    let mut errors = 0;
    for d in v["diagnostics"].as_array().into_iter().flatten() {
        let severity = d["severity"].as_str().unwrap_or("error");
        let code = d["code"].as_str().unwrap_or("");
        errors += usize::from(severity == "error");
        writeln!(
            out,
            "{}:{}: {severity}[{code}]: {}",
            crate::cart::shown(base, d["file"].as_str().unwrap_or("?")),
            d["line"],
            d["message"].as_str().unwrap_or("")
        )?;
        for f in d["fixes"].as_array().into_iter().flatten() {
            let basis = if f["basis"] == "contract" {
                " (contract)"
            } else {
                ""
            };
            writeln!(out, "  fix: {}{basis}", f["label"].as_str().unwrap_or(""))?;
            for e in f["edits"].as_array().into_iter().flatten() {
                let at = match e["replace"].as_u64().unwrap_or(0) {
                    0 => "insert before".to_string(),
                    n => format!("replace {n} line(s) at"),
                };
                writeln!(
                    out,
                    "    {at} {}:{}: {}",
                    e["file"].as_str().unwrap_or("?"),
                    e["line"],
                    e["text"].as_str().unwrap_or("").trim()
                )?;
            }
        }
        if !code.is_empty() {
            writeln!(
                out,
                "{}",
                ppu_rules::docs::pointer(code)
            )?;
        }
    }
    if v["ok"] != true {
        bail!("the cartridge refuses this program: {errors} error(s) above");
    }
    let stop = &v["run"]["stop"];
    if stop.is_object() {
        let message = stop["message"].as_str().unwrap_or("");
        let code = ppu_core::engine_code(message).unwrap_or("snes.halt");
        let when = match (stop["update"].as_u64(), stop["frame"].as_u64()) {
            (Some(u), Some(f)) => format!(
                "at update {u} (frame {f}, scene {})",
                stop["scene"].as_str().unwrap_or("?")
            ),
            _ => "at power-on".into(),
        };
        let file = stop["file"]
            .as_str()
            .or(stop["scene"].as_str())
            .unwrap_or("?");
        writeln!(
            out,
            "{}:{}: error[{code}]: the exported checked program halts {when}: {message}",
            crate::cart::shown(base, file),
            stop["line"].as_u64().unwrap_or(1)
        )?;
        writeln!(
            out,
            "{}",
            ppu_rules::docs::pointer(code)
        )?;
        bail!("a cartridge stops there, where the preview runs on");
    }
    writeln!(
        out,
        "PASS: cartridge check, {} frames run",
        v["run"]["frames"].as_u64().unwrap_or(0)
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_become_plan_edges() {
        // one pad 1 button held 10..20, pad 2's first button 15..30
        let p = plan(&[(1 << 8, 10, 20), (1 << 16, 15, 30)]);
        let frames: Vec<_> = p
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["frame"].as_u64().unwrap())
            .collect();
        assert_eq!(frames, [10, 20, 15, 30]);
        assert_eq!(p[1]["buttons"], json!([]));
        assert_eq!(p[1]["buttons2"], json!([ppu_core::PAD_NAMES[0]]));
        assert_eq!(p[3], json!({"frame": 30, "buttons": [], "buttons2": []}));
    }

    #[test]
    fn a_refusal_points_at_its_note() {
        let v = json!({"ok": false, "error": null, "diagnostics": [
            {"severity": "error", "code": "snes.boot_time", "file": "main.luau", "line": 3, "message": "slow boot"}
        ]});
        let mut out = Vec::new();
        assert!(report(Path::new("."), &v, &mut out).is_err());
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "main.luau:3: error[snes.boot_time]: slow boot\n  docs: ppu docs snes.boot_time\n"
        );
    }

    #[test]
    fn import_names_lose_their_build_hash() {
        assert_eq!(base_name("__wbg_crypto_38df2bab126b63dc"), "crypto");
        assert_eq!(
            base_name("__wbg___wbindgen_throw_344f42d3211c4765"),
            "__wbindgen_throw"
        );
        assert_eq!(
            base_name("__wbindgen_cast_0000000000000001"),
            "__wbindgen_cast_0000000000000001"
        );
        assert_eq!(
            base_name("__wbindgen_init_externref_table"),
            "__wbindgen_init_externref_table"
        );
    }

    #[test]
    fn a_damaged_compiled_module_is_not_trusted() {
        let sealed = seal(b"compiled".to_vec());
        assert_eq!(unseal(&sealed), Some(&b"compiled"[..]));
        let mut flipped = sealed.clone();
        flipped[3] ^= 1;
        assert_eq!(unseal(&flipped), None);
        assert_eq!(unseal(&sealed[..sealed.len() - 1]), None);
        assert_eq!(unseal(b"short"), None);
    }

    #[test]
    fn hashes_are_file_names() {
        assert!(valid_hash("Ab12_xY9-"));
        assert!(!valid_hash("../x") && !valid_hash(""));
    }
}
