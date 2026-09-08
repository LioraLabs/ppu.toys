use anyhow::{bail, ensure, Context, Result};
use std::path::{Path, PathBuf};

const HELP: &str = "ppu — standalone ppu.toys authoring (60 fps, 256×224)

  ppu new <dir>
  ppu pack <dir> [-o out.ppu.json]
  ppu unpack <file.ppu.json> <dir>
  ppu check <dir|file.ppu.json> [--duration seconds] [--seek seconds,...]
            [--loop seconds] [--allow-overflow]
  ppu render <dir|file.ppu.json> --at seconds,... -o <directory>
  ppu docs [topic|all]
  ppu --version

check defaults to 30 seconds; includes the endpoint. --seek compares direct
and backward seeks with playback. --loop adds loop comparisons at sample times.
render plays from zero to the last requested time and writes numbered PNGs.
Times round to the nearest 60 Hz frame. PNG sources convert on every load.
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
    if command == "docs" {
        ensure!(args.len() <= 1, "usage: ppu docs [topic|all]");
        print!("{}", ppu_cli::docs(args.first().map(String::as_str))?);
        return Ok(());
    }
    ensure!(
        !args.is_empty(),
        "{command} requires a path; use ppu --help"
    );
    let input = PathBuf::from(args.remove(0));
    match command.as_str() {
        "new" => {
            ensure!(args.is_empty(), "usage: ppu new <dir>");
            ppu_cli::new_project(&input)?;
            println!("Created {}. Edit main.lua and assets/floor.png; run ppu docs cli for the authoring loop.", input.display());
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
            ensure!(args.is_empty(), "unknown check arguments: {args:?}");
            ppu_cli::check(&input, duration, &seeks, period, allow_overflow)?;
        }
        "render" => {
            let times = option(&mut args, "--at")?.context("render requires --at seconds,...")?;
            let output = option(&mut args, "-o")?.context("render requires -o <directory>")?;
            ensure!(args.is_empty(), "unknown render arguments: {args:?}");
            ppu_cli::render(&input, &ppu_cli::parse_times(&times)?, Path::new(&output))?;
        }
        _ => bail!("unknown command {command:?}; use ppu --help"),
    }
    Ok(())
}
