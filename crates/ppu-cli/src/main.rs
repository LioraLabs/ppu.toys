use anyhow::{bail, ensure, Context, Result};
use std::path::{Path, PathBuf};

const HELP: &str = "ppu — standalone ppu.toys authoring (60 fps, 256×224)

  ppu new <dir> [--lua | --world]
  ppu types <dir>
  ppu skills [dir] [--reset]
  ppu pack <dir> [-o out.ppu.json]
  ppu unpack <file.ppu.json> <dir>
  ppu check <dir|file.ppu.json> [--duration seconds] [--seek seconds,...]
            [--loop seconds] [--allow-overflow] [--hold r@450-460[,a+l@600-610]]
  ppu render <dir|file.ppu.json> --at seconds,... -o <directory> [--hold ...] [--srm save.srm]
  ppu render <dir|file.ppu.json> --wav out.wav --duration seconds [--hold ...]
  ppu convert <dir|file.ppu.json> -o <dir|file.ppu.json>
  ppu song check <file.song.json | file.mid | [dir] song source name>
  ppu song roll <file.song.json | file.mid | [dir] song source name> -o roll.png [--slots a..b]
  ppu art check <png> [--json] [--stdin --width W --height H]
  ppu art palette sync <gpl>
  ppu art new <png> --kind bg|sheet|obj|m7 --size WxH [--bpp 2|4|8] [--cell N]
              (--palette <gpl> | --new-palette <name>)
  ppu art convert <png> [--kind bg|sheet|obj|m7] [converter flags] [--new-palette <name>] [--json]
  ppu art install aseprite|pixelorama [--dir folder]
  ppu sfx coin|jump|hurt|explosion|powerup|blip|laser [-o file.wav] [--wave square|saw|sine|noise]
          [--pitch Hz] [--length ms] [--attack ms] [--decay ms] [--sweep semitones] [--seed N]
  ppu docs [note id|code|error text|words|map] [--links]
  ppu login | whoami | logout
  ppu connect [<project link> <dir>] [--keep folder|site]
  ppu build [dir] [-o out.sfc]
  ppu setup
  ppu --version

check defaults to 30 seconds; includes the endpoint. --seek compares direct
and backward seeks with playback. --loop adds loop comparisons at sample times.
check also runs the cartridge check over the same frames: the site's check module,
downloaded once per version and cached (offline, the cached one); each finding ends
in the ppu docs command for its note.
render plays from zero to the last requested time and writes numbered PNGs;
--srm loads a saveData record's battery RAM image first (when the file exists) and writes it back after.
types rewrites a Luau project's ppu.d.luau and @ppu library stubs after a CLI
upgrade, and adds .luaurc and .vscode/ settings for luau-lsp where missing.
new, connect, types and skills write AGENTS.md and the agent skills (.agents/skills/,
.claude/skills/) this ppu ships; after an upgrade they refresh every file still as
ppu wrote it and keep the ones you edited; skills --reset overwrites those too.
convert turns a Lua toy into a Luau toy (renames, bitwise operators, tab order
as requires) and lists what changed and what it could not convert.
art check reports what importing a project PNG would refuse (exit 1) with tile x,y; --stdin takes raw\nindex bytes. art palette sync rewrites the PLTE of every PNG naming that .gpl. art new makes an empty PNG with its ppu.json source (and a new .gpl); art convert turns a full-colour PNG into SNES art (offline; see ppu docs cli for its flags); art install puts the editor plugin in place. See ppu docs cli.
Times round to the nearest 60 Hz frame. --hold holds buttons on frames [from, to); a 2. prefix (2.a) is the second controller. PNG sources convert on every load.
login signs this machine in to ppu.toys (PPU_SITE=<url> for another site);
whoami prints the account; logout signs it out.
song check prints what the Studio's song_check tool reports: the voice budget and notes it
cuts, rows above C6, held clashes, and loudness in dBFS per arrangement slot and sound, with
clipping (JSON when piped; exit 1 when over the voice budget or above C6). song roll draws a
piano roll PNG: a lane per pitch, highest at the top (C lanes shaded, drums below), one colour
per sound, a line where each arrangement slot starts, cut tails dimmed; --slots 2..4 draws
slots 2 to 4.
connect pulls one of your projects into a new folder and keeps running: each
save in the folder becomes a project save the open Studio picks up, and each
save in the Studio is written into the folder. Inside a linked folder (one with
.ppu/link), ppu connect needs no arguments. What it prints also shows in the
Studio's Connect panel, and Disconnect there stops it. A file changed in both
places is asked about in that panel (keep the folder's or take the site's, whose
replaced copy goes to .ppu-backup/); --keep folder|site settles the ones found
at start without asking.
build sends the folder to the signed-in site's SNES compiler, the Studio's Export to ROM,
prints its diagnostics as file:line and writes the cartridge (build/<folder>-<UTC time>.sfc
inside the folder by default).
setup registers ppu:// links (Linux, macOS, Windows), so the Studio's Connect button starts
ppu connect with no terminal: in a new folder in the ppu folder of your home, or one already
connected to the project. Run with PPU_SITE set, links from that site connect to it.
See ppu docs cli for manifests, examples, and supported features.";

fn option(args: &mut Vec<String>, flag: &str) -> Result<Option<String>> {
    if let Some(i) = args.iter().position(|arg| arg == flag) {
        args.remove(i);
        ensure!(i < args.len(), "{flag} requires a value");
        Ok(Some(args.remove(i)))
    } else {
        Ok(None)
    }
}

fn account(command: &str) -> Result<()> {
    use ppu_cli::login;
    let path = login::credentials_path()?;
    let mut out = std::io::stdout();
    match command {
        "login" => {
            let site = login::site_from_env();
            let creds = login::login(&site, &path, &mut out)?;
            println!("Signed in to {site} as {}.", login::whoami(&creds)?);
            news(&site);
        }
        "whoami" => match login::load(&path)? {
            Some(creds) => println!("{} on {}", login::whoami(&creds)?, creds.site),
            None => bail!("not signed in; run ppu login"),
        },
        _ => {
            if login::logout(&path, &mut out)? {
                println!("Signed out.");
            } else {
                println!("Not signed in.");
            }
        }
    }
    Ok(())
}

/// The site's newest What's new entry on stderr, once per entry, remembered beside the sign-in.
fn news(site: &str) {
    if let Ok(path) = ppu_cli::login::credentials_path() {
        ppu_cli::news::notice(
            site,
            &path.with_file_name("news-seen"),
            &mut std::io::stderr(),
        );
    }
}

/// This machine's sign-in, refused unless it is for the site ppu login would choose: PPU_SITE, or ppu.toys.
fn signed_in() -> Result<ppu_cli::login::Credentials> {
    use ppu_cli::login;
    let creds =
        login::load(&login::credentials_path()?)?.context("not signed in; run ppu login first")?;
    let site = login::site_from_env();
    ensure!(
        creds.site == site,
        "this machine is signed in to {}, not {site}; run {}",
        creds.site,
        match site == login::DEFAULT_SITE {
            true => "ppu login".to_string(),
            false => login::with_site(&site, "ppu login"),
        }
    );
    Ok(creds)
}

/// `ppu build [dir] [-o out.sfc]`: by default each image is kept as `build/<folder>-<UTC stamp>.sfc`
/// inside the folder.
fn build(mut args: Vec<String>) -> Result<()> {
    use ppu_cli::cart;
    let output = option(&mut args, "-o")?;
    ensure!(args.len() <= 1, "usage: ppu build [dir] [-o out.sfc]");
    let dir = PathBuf::from(args.first().map_or(".", String::as_str));
    let rom = cart::build(&signed_in()?, &dir, &mut std::io::stdout())?;
    let output = match output {
        Some(o) => PathBuf::from(o),
        None => {
            let full = dir
                .canonicalize()
                .with_context(|| format!("cannot open {}", dir.display()))?;
            let name = full.file_name().and_then(|s| s.to_str()).unwrap_or("toy");
            let build = dir.join(cart::BUILD_DIR);
            std::fs::create_dir_all(&build)
                .with_context(|| format!("cannot make {}", build.display()))?;
            std::fs::write(build.join(".gitignore"), "*\n")?;
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
            build.join(format!("{name}-{}.sfc", cart::stamp(now.as_secs())))
        }
    };
    std::fs::write(&output, &rom).with_context(|| format!("cannot write {}", output.display()))?;
    println!("Built {} ({} KiB).", output.display(), rom.len() / 1024);
    Ok(())
}

fn connect(mut args: Vec<String>) -> Result<()> {
    use ppu_cli::connect::{self, Clashes, Keep};
    // Live clashes, and every clash without --keep: asked in the Studio's Connect panel, the same
    // whether a terminal or a ppu:// link started connect.
    let live = Clashes::Panel;
    let clashes = match option(&mut args, "--keep")?.as_deref() {
        Some("folder") => Clashes::AtStart(Keep::Folder, Box::new(live)),
        Some("site") => Clashes::AtStart(Keep::Site, Box::new(live)),
        Some(other) => bail!("--keep takes folder or site, not {other:?}"),
        None => live,
    };
    let creds = signed_in()?;
    news(&creds.site);
    let mut out = std::io::stdout();
    // one connect per project, whether a terminal or a ppu:// link started it
    let project = match args.as_slice() {
        [] => connect::Link::read(Path::new("."))?.project,
        [link, _] => connect::project_id(link)?,
        _ => String::new(), // the usage error below
    };
    let _one = match project.is_empty() {
        true => None,
        false => Some(
            connect::hold(&connect::project_lock(&project)?)?.with_context(|| {
                format!(
                    "ppu connect for {project} is already running (from a terminal or the Studio)"
                )
            })?,
        ),
    };
    let dir = match args.as_slice() {
        [] => PathBuf::from("."),
        [link, dir] => {
            let dir = ppu_cli::login::expand_home(dir);
            let link = connect::pull(&creds, link, &dir, &mut out)?;
            println!(
                "Pulled project {} (revision {}) into {}.",
                link.project,
                link.revision,
                dir.display()
            );
            dir
        }
        _ => {
            bail!("usage: ppu connect <project link> <dir>, or ppu connect inside a linked folder")
        }
    };
    // connect writes AGENTS.md and the skills itself, so the Connect panel's log hears it too
    connect::connect(&creds, &dir, clashes, &mut out)
}

/// Write AGENTS.md and the skills, naming each file kept (and, when `list`, each written).
fn skills(dir: &Path, reset: bool, list: bool) -> Result<()> {
    let done = ppu_cli::write_skills(dir, reset)?;
    for name in done.written.iter().filter(|_| list) {
        println!("Wrote {name}");
    }
    for name in &done.kept {
        println!("Kept {name}: edited since ppu wrote it (ppu skills --reset replaces it)");
    }
    Ok(())
}

fn song(mut args: Vec<String>) -> Result<()> {
    use ppu_cli::song;
    let usage = "usage: ppu song check <file.song.json | file.mid | [dir] name>\n       ppu song roll <file.song.json | file.mid | [dir] name> -o roll.png [--slots a..b]";
    ensure!(!args.is_empty(), "{usage}");
    match args.remove(0).as_str() {
        "check" => {
            ensure!(!args.iter().any(|a| a.starts_with('-')), "{usage}");
            if song::check(&song::load(&args)?)? {
                std::process::exit(1);
            }
            Ok(())
        }
        "roll" => {
            let out = option(&mut args, "-o")?.context("song roll requires -o roll.png")?;
            let slots = option(&mut args, "--slots")?
                .map(|s| song::parse_slots(&s))
                .transpose()?;
            ensure!(!args.iter().any(|a| a.starts_with('-')), "{usage}");
            let loaded = song::load(&args)?;
            let roll = ppu_core::song_roll::roll(&loaded.song, slots).map_err(|e| anyhow::anyhow!("--{e}"))?;
            song::write_png(&roll, Path::new(&out))?;
            println!("{out}: {}x{}", roll.width, roll.height);
            Ok(())
        }
        _ => bail!("{usage}"),
    }
}

fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return Ok(());
    }
    if args == ["--version"] || args == ["-V"] {
        println!("ppu {} (ppu.toys/1)", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let command = args.remove(0);
    if command.starts_with("ppu:") {
        ensure!(args.is_empty(), "usage: ppu <ppu:// link>");
        return ppu_cli::link::run(&command, &mut std::io::stdout());
    }
    if command == "setup" {
        ensure!(args.is_empty(), "usage: ppu setup");
        return ppu_cli::setup::setup(&mut std::io::stdout());
    }
    if command == "docs" {
        let links = args.iter().any(|a| a == "--links");
        let words: Vec<&str> = args
            .iter()
            .map(String::as_str)
            .filter(|a| *a != "--links")
            .collect();
        ensure!(
            !words.iter().any(|a| a.starts_with("--")),
            "usage: ppu docs [query] [--links]"
        );
        let query = (!words.is_empty()).then(|| words.join(" "));
        print!("{}", ppu_cli::docs(query.as_deref(), links)?);
        return Ok(());
    }
    if matches!(command.as_str(), "login" | "whoami" | "logout") {
        ensure!(args.is_empty(), "usage: ppu {command}");
        return account(&command);
    }
    if command == "connect" {
        return connect(args);
    }
    if command == "build" {
        return build(args);
    }
    if command == "skills" {
        let reset = args.iter().any(|a| a == "--reset");
        args.retain(|a| a != "--reset");
        ensure!(args.len() <= 1, "usage: ppu skills [dir] [--reset]");
        return skills(
            Path::new(args.first().map_or(".", String::as_str)),
            reset,
            true,
        );
    }
    if command == "art" {
        std::process::exit(ppu_cli::art::run(args));
    }
    if command == "sfx" {
        use ppu_core::sfx;
        let request = sfx::parse(&args).map_err(anyhow::Error::msg)?;
        let (wav, stats) =
            sfx::make(request.preset, &request.knobs).map_err(anyhow::Error::msg)?;
        let mut stats = stats.to_json();
        let preset = stats["preset"].as_str().unwrap_or("sfx");
        let out = request.out.unwrap_or_else(|| format!("{preset}.wav"));
        std::fs::write(&out, wav).with_context(|| format!("cannot write {out}"))?;
        stats["out"] = out.into();
        println!("{stats}");
        return Ok(());
    }
    if command == "song" {
        return song(args);
    }
    ensure!(
        !args.is_empty(),
        "{command} requires a path; use ppu --help"
    );
    let input = PathBuf::from(args.remove(0));
    match command.as_str() {
        "new" if args.iter().any(|a| a == "--world") => {
            ensure!(
                args == ["--world"],
                "usage: ppu new <dir> --world (a Luau world toy; no other options)"
            );
            ppu_cli::new_world_project(&input)?;
            println!("Created {}. Open project.tiled-project in Tiled, paint level1.tmj and save; edit main.luau; run ppu docs worlds.", input.display());
        }
        "new" => {
            let lua = args.iter().any(|a| a == "--lua");
            args.retain(|a| a != "--lua");
            ensure!(args.is_empty(), "usage: ppu new <dir> [--lua]");
            ppu_cli::new_project(&input, lua)?;
            let main = if lua {
                "main.lua"
            } else {
                "main.luau (with luau-lsp types: .luaurc, ppu.d.luau, .vscode/)"
            };
            println!("Created {}. Edit {main} and assets/floor.png; run ppu docs cli for the authoring loop.", input.display());
        }
        "types" => {
            ensure!(args.is_empty(), "usage: ppu types <dir>");
            for name in ppu_cli::write_editor_files(&input)? {
                println!("Wrote {name}");
            }
            skills(&input, false, true)?;
        }
        "pack" => {
            let output = option(&mut args, "-o")?.unwrap_or_else(|| {
                format!(
                    "{}.ppu.json",
                    input.file_name().and_then(|s| s.to_str()).unwrap_or("out")
                )
            });
            ensure!(args.is_empty(), "usage: ppu pack <dir> [-o out.ppu.json]");
            std::fs::write(&output, ppu_cli::pack(&input)?)
                .with_context(|| format!("cannot write {output}"))?;
            println!("Packed {output}");
        }
        "unpack" => {
            ensure!(args.len() == 1, "usage: ppu unpack <file> <dir>");
            let text = std::fs::read_to_string(&input)?;
            ppu_cli::unpack(&text, Path::new(&args[0]))?;
            println!("Unpacked into {}", args[0]);
        }
        "check" => {
            let duration = option(&mut args, "--duration")?
                .unwrap_or_else(|| "30".into())
                .parse()
                .context("invalid duration")?;
            let seeks = option(&mut args, "--seek")?
                .map(|s| ppu_cli::parse_times(&s))
                .transpose()?
                .unwrap_or_default();
            let period = option(&mut args, "--loop")?
                .map(|s| s.parse::<f64>())
                .transpose()
                .context("invalid loop period")?;
            let allow_overflow = if let Some(i) = args.iter().position(|a| a == "--allow-overflow")
            {
                args.remove(i);
                true
            } else {
                false
            };
            let holds = option(&mut args, "--hold")?
                .map(|h| ppu_cli::parse_holds(&h))
                .transpose()?
                .unwrap_or_default();
            ensure!(args.is_empty(), "unknown check arguments: {args:?}");
            // the preview and the cartridge check over the same frames, side by side: both always
            // run, and the cartridge's lines print after the preview's
            let last = ppu_cli::frame_number(duration)?;
            let cart = std::thread::spawn({
                let (input, holds) = (input.clone(), holds.clone());
                move || {
                    let mut out = Vec::new();
                    let r = ppu_cli::cart_check::check(&input, last, &holds, &mut out);
                    (out, r)
                }
            });
            let preview = ppu_cli::check(&input, duration, &seeks, period, allow_overflow, &holds);
            let (out, cart) = cart
                .join()
                .map_err(|_| anyhow::anyhow!("the cartridge check panicked"))?;
            std::io::Write::write_all(&mut std::io::stdout(), &out)?;
            if let (Err(_), Err(e)) = (&preview, &cart) {
                eprintln!("error: {e:#}");
            }
            preview?;
            cart?;
        }
        "render" if args.iter().any(|a| a == "--wav") => {
            let output = option(&mut args, "--wav")?.context("--wav requires a file")?;
            let duration = option(&mut args, "--duration")?
                .context("render --wav requires --duration seconds")?
                .parse::<f64>()
                .context("invalid duration")?;
            let holds = option(&mut args, "--hold")?
                .map(|h| ppu_cli::parse_holds(&h))
                .transpose()?
                .unwrap_or_default();
            ensure!(
                args.is_empty(),
                "render --wav takes --duration and --hold only (--at is the PNG mode): {args:?}"
            );
            ppu_cli::render_wav(
                &input,
                ppu_cli::frame_number(duration)?,
                Path::new(&output),
                &holds,
            )?;
        }
        "render" => {
            let times = option(&mut args, "--at")?.context("render requires --at seconds,...")?;
            let output = option(&mut args, "-o")?.context("render requires -o <directory>")?;
            let holds = option(&mut args, "--hold")?
                .map(|h| ppu_cli::parse_holds(&h))
                .transpose()?
                .unwrap_or_default();
            let srm = option(&mut args, "--srm")?.map(PathBuf::from);
            ensure!(args.is_empty(), "unknown render arguments: {args:?}");
            ppu_cli::render(
                &input,
                &ppu_cli::parse_times(&times)?,
                Path::new(&output),
                &holds,
                srm.as_deref(),
            )?;
        }
        "convert" => {
            let output =
                option(&mut args, "-o")?.context("convert requires -o <dir|file.ppu.json>")?;
            ensure!(
                args.is_empty(),
                "usage: ppu convert <dir|file.ppu.json> -o <dir|file.ppu.json>"
            );
            let c = ppu_cli::convert(&input, Path::new(&output))?;
            for n in &c.changes {
                println!("{}:{}: {}", n.file, n.line, n.message);
            }
            for n in &c.unconverted {
                println!("{}:{}: not converted: {}", n.file, n.line, n.message);
            }
            println!(
                "Converted into {output}: {} changes, {} not converted.",
                c.changes.len(),
                c.unconverted.len()
            );
        }
        _ => bail!("unknown command {command:?}; use ppu --help"),
    }
    Ok(())
}
