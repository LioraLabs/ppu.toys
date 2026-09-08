use anyhow::{bail, Result};

// Embed the maintained application docs: installed binaries need no checkout.
const TOPICS: &[(&str, &str)] = &[
    ("cli", include_str!("../../../docs/cli.md")),
    ("registers", include_str!("../../../docs/registers.md")),
    ("display", include_str!("../../../docs/display.md")),
    ("backgrounds", include_str!("../../../docs/backgrounds.md")),
    ("sprites", include_str!("../../../docs/sprites.md")),
    ("mode7", include_str!("../../../docs/mode7.md")),
    ("scanlines", include_str!("../../../docs/scanlines.md")),
    ("windows", include_str!("../../../docs/windows.md")),
    ("color-math", include_str!("../../../docs/color-math.md")),
    ("sources", include_str!("../../../docs/sources.md")),
    ("dma", include_str!("../../../docs/dma.md")),
    ("pad", include_str!("../../../docs/pad.md")),
];

pub fn docs(topic: Option<&str>) -> Result<String> {
    match topic {
        None => Ok(format!("Offline documentation bundled with ppu {}.\nUse ppu docs <topic> (Markdown links refer to other topics).\nTopics: {}, all\n", env!("CARGO_PKG_VERSION"), TOPICS.iter().map(|t| t.0).collect::<Vec<_>>().join(", "))),
        Some("all") => Ok(TOPICS.iter().map(|t| t.1).collect::<Vec<_>>().join("\n\n")),
        Some(name) => match TOPICS.iter().find(|t| t.0 == name) {
            Some((_, text)) => Ok((*text).into()),
            None => bail!("unknown documentation topic {name:?}; use ppu docs"),
        },
    }
}
