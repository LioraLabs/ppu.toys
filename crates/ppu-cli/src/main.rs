use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

fn default_out(dir: &Path) -> PathBuf {
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("out");
    PathBuf::from(format!("{name}.ppu.json"))
}

fn usage() -> ! {
    eprintln!("usage:\n  ppu pack <dir> [-o out.ppu.json]\n  ppu unpack <file> <dir>");
    std::process::exit(2)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["pack", dir] => {
            let dir = Path::new(dir);
            let out = default_out(dir);
            std::fs::write(&out, ppu_cli::pack(dir)?)
                .with_context(|| format!("cannot write {}", out.display()))?;
            println!("Packed {}", out.display());
        }
        ["pack", dir, "-o", out] => {
            let dir = Path::new(dir);
            let out = Path::new(out);
            std::fs::write(out, ppu_cli::pack(dir)?)
                .with_context(|| format!("cannot write {}", out.display()))?;
            println!("Packed {}", out.display());
        }
        ["unpack", file, dir] => {
            let text =
                std::fs::read_to_string(file).with_context(|| format!("cannot read {file}"))?;
            ppu_cli::unpack(&text, Path::new(dir))?;
            println!("Unpacked into {dir}");
        }
        _ => usage(),
    }
    Ok(())
}
