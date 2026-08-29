use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use reqwest::{Client, Response, StatusCode, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MANIFEST: &str = "ppu.json";
const STARTER: &str = "function frame(t, f)\n  brightness = 15\n  -- Both screens power on empty: designate a layer to see it,\n  -- e.g. screen.main.bg1 = true (TM) after pointing bg[1] at VRAM.\nend\n";

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct FileDto {
    name: String,
    source: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceDto {
    name: String,
    kind: String,
    #[serde(default)]
    builtin_id: Option<String>,
    #[serde(default)]
    options: serde_json::Value,
    #[serde(default)]
    meta: serde_json::Value,
    #[serde(default)]
    payload: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ManifestSource {
    name: String,
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    builtin_id: Option<String>,
    #[serde(default)]
    options: serde_json::Value,
    #[serde(default)]
    meta: serde_json::Value,
    payload: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Toy {
    id: String,
    revision: i64,
    title: String,
    description: String,
    files: Vec<FileDto>,
    sources: Vec<SourceDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    server: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    toy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    revision: Option<i64>,
    title: String,
    #[serde(default)]
    description: String,
    files: Vec<String>,
    #[serde(default)]
    sources: Vec<ManifestSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    clip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thumb: Option<String>,
    #[serde(default)]
    hashes: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize)]
struct Config {
    server: String,
    token: String,
}

fn config_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("PPU_CONFIG") {
        return Ok(path.into());
    }
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("set HOME or PPU_CONFIG"))?;
    Ok(PathBuf::from(home).join(".config/ppu/config.json"))
}

fn read_config() -> Result<Config> {
    serde_json::from_slice(&std::fs::read(config_path()?).context("run `ppu login <token>` first")?)
        .context("invalid ppu config")
}

fn read_manifest(dir: &Path) -> Result<Manifest> {
    serde_json::from_slice(
        &std::fs::read(dir.join(MANIFEST)).context("no ppu.json in this directory")?,
    )
    .context("invalid ppu.json")
}

fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<()> {
    std::fs::write(dir.join(MANIFEST), serde_json::to_vec_pretty(manifest)?)?;
    Ok(())
}

fn valid_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != MANIFEST
        && Path::new(name).components().count() == 1
        && !matches!(name, "." | "..")
}

fn safe_relative(name: &str) -> bool {
    !name.is_empty()
        && Path::new(name).components().all(|part| {
            matches!(
                part,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
}

fn api(server: &str, path: &str) -> String {
    format!("{}/api{}", server.trim_end_matches('/'), path)
}
fn hash(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

fn local_files(dir: &Path, manifest: &Manifest) -> Result<Vec<FileDto>> {
    manifest
        .files
        .iter()
        .map(|name| {
            if !valid_file_name(name) {
                bail!("unsafe file name {name:?}");
            }
            Ok(FileDto {
                name: name.clone(),
                source: std::fs::read_to_string(dir.join(name))
                    .with_context(|| format!("cannot read {name}"))?,
            })
        })
        .collect()
}

fn local_sources(dir: &Path, manifest: &Manifest) -> Result<Vec<SourceDto>> {
    manifest
        .sources
        .iter()
        .map(|source| {
            if !safe_relative(&source.payload) {
                bail!("unsafe source payload path {:?}", source.payload);
            }
            Ok(SourceDto {
                name: source.name.clone(),
                kind: source.kind.clone(),
                builtin_id: source.builtin_id.clone(),
                options: source.options.clone(),
                meta: source.meta.clone(),
                payload: Some(BASE64.encode(
                    std::fs::read(dir.join(&source.payload)).with_context(|| {
                        format!("cannot read source payload {}", source.payload)
                    })?,
                )),
            })
        })
        .collect()
}

fn hashes(files: &[FileDto]) -> BTreeMap<String, String> {
    files
        .iter()
        .map(|file| (file.name.clone(), hash(&file.source)))
        .collect()
}

fn local_changed(dir: &Path, manifest: &Manifest) -> Result<bool> {
    Ok(hashes(&local_files(dir, manifest)?) != manifest.hashes)
}

async fn response_error(response: Response, action: &str) -> Result<Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    bail!("{action} failed ({status}): {text}")
}

async fn fetch_toy(client: &Client, config: &Config, id: &str) -> Result<Toy> {
    response_error(
        client
            .get(api(&config.server, &format!("/toys/{id}")))
            .bearer_auth(&config.token)
            .send()
            .await?,
        "fetch",
    )
    .await?
    .json()
    .await
    .context("invalid toy response")
}

fn apply_remote(dir: &Path, manifest: &mut Manifest, toy: Toy) -> Result<()> {
    if let Some(file) = toy.files.iter().find(|file| !valid_file_name(&file.name)) {
        bail!("toy contains unsafe file name {:?}", file.name);
    }
    for old in &manifest.files {
        if !toy.files.iter().any(|file| &file.name == old) && dir.join(old).exists() {
            std::fs::remove_file(dir.join(old))?;
        }
    }
    for file in &toy.files {
        std::fs::write(dir.join(&file.name), &file.source)?;
    }
    let source_dir = dir.join(".ppu/sources");
    std::fs::create_dir_all(&source_dir)?;
    let mut sources = Vec::with_capacity(toy.sources.len());
    for (i, source) in toy.sources.iter().enumerate() {
        let payload_path = format!(".ppu/sources/{i}.bin");
        let payload = source
            .payload
            .as_deref()
            .ok_or_else(|| anyhow!("source {:?} has no payload", source.name))?;
        std::fs::write(
            dir.join(&payload_path),
            BASE64.decode(payload).context("bad source payload")?,
        )?;
        sources.push(ManifestSource {
            name: source.name.clone(),
            kind: source.kind.clone(),
            builtin_id: source.builtin_id.clone(),
            options: source.options.clone(),
            meta: source.meta.clone(),
            payload: payload_path,
        });
    }
    manifest.toy = Some(toy.id);
    manifest.revision = Some(toy.revision);
    manifest.title = toy.title;
    manifest.description = toy.description;
    manifest.files = toy.files.iter().map(|file| file.name.clone()).collect();
    manifest.sources = sources;
    manifest.hashes = hashes(&toy.files);
    write_manifest(dir, manifest)
}

async fn login(token: &str, server: &str) -> Result<()> {
    let server = server.trim_end_matches('/');
    response_error(
        Client::new()
            .get(api(server, "/me"))
            .bearer_auth(token)
            .send()
            .await?,
        "login",
    )
    .await?;
    let path = config_path()?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&Config {
            server: server.into(),
            token: token.into(),
        })?,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    println!("Logged in to {server}");
    Ok(())
}

fn new_toy(directory: &str) -> Result<()> {
    let server = read_config()
        .map(|config| config.server)
        .unwrap_or_else(|_| "https://ppu.toys".into());
    let dir = PathBuf::from(directory);
    if dir.exists() {
        bail!("{} already exists", dir.display());
    }
    std::fs::create_dir(&dir)?;
    std::fs::write(dir.join("main.lua"), STARTER)?;
    let files = vec![FileDto {
        name: "main.lua".into(),
        source: STARTER.into(),
    }];
    let title = dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("untitled toy")
        .to_string();
    write_manifest(
        &dir,
        &Manifest {
            server,
            toy: None,
            revision: None,
            title,
            description: String::new(),
            files: vec!["main.lua".into()],
            sources: vec![],
            clip: None,
            thumb: None,
            hashes: hashes(&files),
        },
    )?;
    println!(
        "Created {}. Run `ppu push` inside it to create a remote draft.",
        dir.display()
    );
    Ok(())
}

fn parse_toy_url(value: &str) -> Result<(String, String)> {
    let url = Url::parse(value).context("expected a full toy URL")?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        bail!("expected an http(s) toy URL");
    }
    let parts: Vec<_> = url.path_segments().into_iter().flatten().collect();
    let id = parts
        .windows(2)
        .find(|p| p[0] == "t")
        .map(|p| p[1].to_string())
        .ok_or_else(|| anyhow!("expected a URL like https://ppu.toys/t/abc123"))?;
    let server = format!(
        "{}://{}{}",
        url.scheme(),
        url.host_str().unwrap(),
        url.port().map(|p| format!(":{p}")).unwrap_or_default()
    );
    Ok((server, id))
}

async fn pull(url: &str, destination: Option<&str>, force: bool) -> Result<()> {
    let (server, toy_id) = parse_toy_url(url)?;
    let config = read_config()?;
    if config.server.trim_end_matches('/') != server.trim_end_matches('/') {
        bail!(
            "logged into {}, but this toy belongs to {server}",
            config.server
        );
    }
    let toy = fetch_toy(&Client::new(), &config, &toy_id).await?;
    let dir = PathBuf::from(destination.unwrap_or(&toy.id));
    if dir.exists() {
        let mut manifest = read_manifest(&dir)?;
        if manifest.toy.as_deref() != Some(&toy_id) {
            bail!("{} belongs to a different toy", dir.display());
        }
        if !force && local_changed(&dir, &manifest)? {
            bail!("local files changed; use `ppu sync` or `ppu pull --force`");
        }
        apply_remote(&dir, &mut manifest, toy)?;
    } else {
        std::fs::create_dir(&dir)?;
        let mut manifest = Manifest {
            server,
            toy: None,
            revision: None,
            title: String::new(),
            description: String::new(),
            files: vec![],
            sources: vec![],
            clip: None,
            thumb: None,
            hashes: BTreeMap::new(),
        };
        apply_remote(&dir, &mut manifest, toy)?;
    }
    println!("Pulled into {}", dir.display());
    Ok(())
}

async fn push(directory: Option<&str>, force: bool, recreate: bool) -> Result<()> {
    let dir = Path::new(directory.unwrap_or("."));
    let mut manifest = read_manifest(dir)?;
    let config = read_config()?;
    if config.server.trim_end_matches('/') != manifest.server.trim_end_matches('/') {
        bail!(
            "logged into {}, but this toy belongs to {}",
            config.server,
            manifest.server
        );
    }
    let files = local_files(dir, &manifest)?;
    let sources = local_sources(dir, &manifest)?;
    let client = Client::new();
    if recreate {
        if let Some(id) = manifest.toy.as_deref() {
            let response = client
                .get(api(&manifest.server, &format!("/toys/{id}")))
                .bearer_auth(&read_config()?.token)
                .send()
                .await?;
            if response.status() == StatusCode::NOT_FOUND {
                manifest.toy = None;
                manifest.revision = None;
            } else {
                response_error(response, "fetch").await?;
            }
        }
    }
    if let Some(id) = manifest.toy.clone() {
        let remote = fetch_toy(&client, &config, &id).await?;
        let expected = if force {
            remote.revision
        } else {
            let base = manifest
                .revision
                .ok_or_else(|| anyhow!("ppu.json has no revision"))?;
            if remote.revision != base {
                bail!(
                    "remote changed from revision {base} to {}; run `ppu sync`",
                    remote.revision
                );
            }
            base
        };
        let response = client
            .put(api(&manifest.server, &format!("/toys/{id}")))
            .bearer_auth(&config.token)
            .json(&serde_json::json!({
                "title": manifest.title, "description": manifest.description, "files": files,
            "sources": sources, "expectedRevision": expected,
            }))
            .send()
            .await?;
        if response.status() == StatusCode::CONFLICT {
            bail!("remote changed while pushing; run `ppu sync`");
        }
        let value: serde_json::Value = response_error(response, "push").await?.json().await?;
        manifest.revision = value["revision"].as_i64();
        manifest.hashes = hashes(&files);
        write_manifest(dir, &manifest)?;
        println!("Pushed {id} at revision {}", manifest.revision.unwrap());
    } else {
        let response = client.post(api(&manifest.server, "/toys")).bearer_auth(&config.token).json(&serde_json::json!({
            "title": manifest.title, "description": manifest.description, "files": files, "sources": sources,
        })).send().await?;
        let value: serde_json::Value = response_error(response, "create").await?.json().await?;
        manifest.toy = value["id"].as_str().map(str::to_string);
        manifest.revision = value["revision"].as_i64();
        manifest.hashes = hashes(&files);
        write_manifest(dir, &manifest)?;
        println!(
            "Created draft: {}/t/{}",
            manifest.server,
            manifest.toy.as_deref().unwrap()
        );
    }
    Ok(())
}

async fn publish(directory: Option<&str>) -> Result<()> {
    let dir = Path::new(directory.unwrap_or("."));
    let manifest = read_manifest(dir)?;
    let id = manifest
        .toy
        .as_deref()
        .ok_or_else(|| anyhow!("push the toy first"))?;
    let clip = manifest
        .clip
        .as_deref()
        .ok_or_else(|| anyhow!("ppu.json has no clip"))?;
    let thumb = manifest
        .thumb
        .as_deref()
        .ok_or_else(|| anyhow!("ppu.json has no thumb"))?;
    if !safe_relative(clip) || !safe_relative(thumb) {
        bail!("unsafe preview path");
    }
    let config = read_config()?;
    if config.server.trim_end_matches('/') != manifest.server.trim_end_matches('/') {
        bail!(
            "logged into {}, but this toy belongs to {}",
            config.server,
            manifest.server
        );
    }
    let form = reqwest::multipart::Form::new()
        .text(
            "meta",
            serde_json::json!({ "title": manifest.title, "description": manifest.description })
                .to_string(),
        )
        .part(
            "clip",
            reqwest::multipart::Part::bytes(std::fs::read(dir.join(clip))?).file_name("clip.webm"),
        )
        .part(
            "thumb",
            reqwest::multipart::Part::bytes(std::fs::read(dir.join(thumb))?).file_name("thumb.png"),
        );
    let response = Client::new()
        .post(api(&manifest.server, &format!("/toys/{id}/publish")))
        .bearer_auth(config.token)
        .multipart(form)
        .send()
        .await?;
    response_error(response, "publish").await?;
    println!("Published {}/t/{id}", manifest.server);
    Ok(())
}

async fn status(directory: Option<&str>) -> Result<()> {
    let dir = Path::new(directory.unwrap_or("."));
    let manifest = read_manifest(dir)?;
    let changed = local_changed(dir, &manifest)?;
    let Some(id) = &manifest.toy else {
        println!(
            "local: {}\nremote: not created",
            if changed { "modified" } else { "new" }
        );
        return Ok(());
    };
    let remote = fetch_toy(&Client::new(), &read_config()?, id).await?;
    println!(
        "local: {}\nremote: {}",
        if changed { "modified" } else { "clean" },
        if Some(remote.revision) == manifest.revision {
            "current"
        } else {
            "changed"
        }
    );
    Ok(())
}

fn write_conflict(dir: &Path, remote: &Toy) -> Result<()> {
    let out = dir.join(".ppu/remote");
    std::fs::create_dir_all(&out)?;
    for file in &remote.files {
        if valid_file_name(&file.name) {
            std::fs::write(out.join(&file.name), &file.source)?;
        }
    }
    println!(
        "Local and remote both changed. Remote files are in {}",
        out.display()
    );
    Ok(())
}

async fn sync(directory: Option<&str>) -> Result<()> {
    let dir = Path::new(directory.unwrap_or("."));
    let mut manifest = read_manifest(dir)?;
    if manifest.toy.is_none() {
        return push(directory, false, false).await;
    }
    let local = local_changed(dir, &manifest)?;
    let remote = fetch_toy(
        &Client::new(),
        &read_config()?,
        manifest.toy.as_deref().unwrap(),
    )
    .await?;
    match (local, Some(remote.revision) != manifest.revision) {
        (false, false) => println!("Already in sync"),
        (true, false) => return push(directory, false, false).await,
        (false, true) => {
            apply_remote(dir, &mut manifest, remote)?;
            println!("Pulled remote changes");
        }
        (true, true) => {
            write_conflict(dir, &remote)?;
            bail!("sync stopped: resolve the conflict, then push");
        }
    }
    Ok(())
}

fn usage() -> ! {
    eprintln!("usage:\n  ppu login <token> [server]\n  ppu new <directory>\n  ppu status [directory]\n  ppu pull <toy-url> [directory] [--force]\n  ppu push [directory] [--force] [--recreate]\n  ppu sync [directory]\n  ppu publish [directory]");
    std::process::exit(2)
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let force = args.iter().any(|arg| arg == "--force");
    let recreate = args.iter().any(|arg| arg == "--recreate");
    args.retain(|arg| arg != "--force");
    args.retain(|arg| arg != "--recreate");
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["login", token] => login(token, "https://ppu.toys").await,
        ["login", token, server] => login(token, server).await,
        ["new", dir] => new_toy(dir),
        ["status"] => status(None).await,
        ["status", dir] => status(Some(dir)).await,
        ["pull", url] => pull(url, None, force).await,
        ["pull", url, dir] => pull(url, Some(dir), force).await,
        ["push"] => push(None, force, recreate).await,
        ["push", dir] => push(Some(dir), force, recreate).await,
        ["sync"] => sync(None).await,
        ["sync", dir] => sync(Some(dir)).await,
        ["publish"] => publish(None).await,
        ["publish", dir] => publish(Some(dir)).await,
        _ => usage(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_local_changes_and_safe_names() {
        assert!(valid_file_name("main.lua"));
        assert!(!valid_file_name("../main.lua"));
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.lua"), "one").unwrap();
        let mut manifest = Manifest {
            server: "http://test".into(),
            toy: None,
            revision: None,
            title: "t".into(),
            description: String::new(),
            files: vec!["main.lua".into()],
            sources: vec![],
            clip: None,
            thumb: None,
            hashes: BTreeMap::new(),
        };
        manifest.hashes = hashes(&local_files(dir.path(), &manifest).unwrap());
        assert!(!local_changed(dir.path(), &manifest).unwrap());
        std::fs::write(dir.path().join("main.lua"), "two").unwrap();
        assert!(local_changed(dir.path(), &manifest).unwrap());
    }

    #[test]
    fn reads_manifest_source_payloads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("payload.bin"), [1, 2, 3]).unwrap();
        let manifest = Manifest {
            server: "http://test".into(),
            toy: None,
            revision: None,
            title: "t".into(),
            description: String::new(),
            files: vec![],
            clip: None,
            thumb: None,
            hashes: BTreeMap::new(),
            sources: vec![ManifestSource {
                name: "art".into(),
                kind: "bg".into(),
                builtin_id: None,
                options: serde_json::json!({}),
                meta: serde_json::json!({}),
                payload: "payload.bin".into(),
            }],
        };
        assert_eq!(
            local_sources(dir.path(), &manifest).unwrap()[0]
                .payload
                .as_deref(),
            Some("AQID")
        );
    }
}
