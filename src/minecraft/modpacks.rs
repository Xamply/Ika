use super::{engine, instance_catalog::*, instances::version_token};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use sha2::Sha512;
use std::{
    collections::HashMap,
    fmt::Write as _,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Provider {
    CurseForge,
    Modrinth,
}

impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Self::CurseForge => "CurseForge",
            Self::Modrinth => "Modrinth",
        }
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Project {
    pub provider: Provider,
    pub id: String,
    pub title: String,
    pub author: String,
    pub description: String,
    pub downloads: u64,
    pub icon: String,
    pub gallery: Vec<String>,
    pub web_url: String,
    curse_loaders: Vec<(u64, String, String)>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Release {
    id: String,
    pub title: String,
    pub minecraft: String,
    pub loader: String,
}

pub struct Details {
    pub description: String,
    pub gallery: Vec<String>,
    pub gallery_thumbnails: HashMap<String, String>,
    pub body: Vec<DescriptionBlock>,
    pub versions: Result<Vec<Release>, String>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub enum DescriptionBlock {
    Line {
        heading: u8,
        fragments: Vec<DescriptionText>,
    },
    Image {
        url: String,
        alt: String,
        link: Option<String>,
        #[serde(default)]
        heading: u8,
    },
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DescriptionText {
    pub text: String,
    pub bold: bool,
    pub link: Option<String>,
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

fn project(provider: Provider, v: &Value) -> Option<Project> {
    let curse = provider == Provider::CurseForge;
    let id = if curse {
        v["id"].as_u64()?.to_string()
    } else {
        v["project_id"].as_str()?.into()
    };
    let slug = s(v, "slug");
    let title = s(v, if curse { "name" } else { "title" });
    if title.is_empty() || id.is_empty() {
        return None;
    }
    let img = if curse {
        s(&v["logo"], "thumbnailUrl")
    } else {
        s(v, "icon_url")
    };
    let curse_loaders = items(&v["latestFilesIndexes"])
        .filter_map(|entry| {
            let loader = match entry["modLoader"].as_u64()? {
                1 => "Forge",
                4 => "Fabric",
                5 => "Quilt",
                6 => "NeoForge",
                _ => return None,
            };
            Some((
                entry["fileId"].as_u64().unwrap_or(0),
                s(entry, "gameVersion").into(),
                loader.into(),
            ))
        })
        .collect();
    Some(Project {
        provider,
        id,
        title: title.into(),
        author: if curse {
            s(&v["authors"][0], "name")
        } else {
            s(v, "author")
        }
        .into(),
        description: s(v, if curse { "summary" } else { "description" }).into(),
        downloads: v[if curse { "downloadCount" } else { "downloads" }]
            .as_u64()
            .unwrap_or(0),
        icon: img.into(),
        gallery: gallery(&v[if curse { "screenshots" } else { "gallery" }]),
        curse_loaders,
        web_url: if curse {
            format!(
                "https://www.curseforge.com/minecraft/modpacks/{}",
                encode(slug)
            )
        } else {
            format!("https://modrinth.com/modpack/{}", encode(slug))
        },
    })
}

pub fn search(provider: Provider, query: &str, mc: &str) -> Result<Vec<Project>, String> {
    let query = encode(query);
    let mut results = Vec::new();
    match provider {
        Provider::Modrinth => {
            let mut facets = vec![vec!["project_type:modpack".to_owned()]];
            if !mc.is_empty() {
                facets.push(vec![format!("versions:{mc}")]);
            }
            let facets = encode(&serde_json::to_string(&facets).unwrap());
            let url =
                format!("{MODRINTH}/search?query={query}&facets={facets}&index=downloads&limit=90");
            let data = json(agent().get(&url).call())?;
            results.extend(items(&data["hits"]).filter_map(|v| project(provider, v)));
        }
        Provider::CurseForge => {
            let key = cf_key()?;
            let client = cf_agent();
            let mut endpoint = format!(
                "mods/search?gameId=432&classId=4471&sortField=6&sortOrder=desc&pageSize=50&searchFilter={query}"
            );
            if !mc.is_empty() {
                let _ = write!(endpoint, "&gameVersion={}", encode(mc));
            }
            let page = cf_json(&client, &key, &format!("{endpoint}&index=0"))?;
            results.extend(items(&page["data"]).filter_map(|v| project(provider, v)));
            if page["pagination"]["totalCount"].as_u64().unwrap_or(0) > 50
                && let Ok(page) = cf_json(&client, &key, &format!("{endpoint}&index=50"))
            {
                for entry in items(&page["data"]).filter_map(|v| project(provider, v)) {
                    if !results.iter().any(|v| v.id == entry.id) {
                        results.push(entry);
                    }
                }
            }
        }
    }
    results.truncate(90);
    Ok(results)
}

fn gallery(value: &Value) -> Vec<String> {
    let mut entries = items(value).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry["featured"].as_bool() != Some(true));
    entries
        .into_iter()
        .map(|v| v.as_str().unwrap_or_else(|| s(v, "url")))
        .filter(|u| https(u))
        .take(24)
        .map(str::to_owned)
        .collect()
}

fn html_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

pub fn description_url(url: &str, curse: bool) -> Option<String> {
    let url = html_entities(url.trim());
    if !url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        if https(&url) {
            return Some(url);
        }
        if curse && url.starts_with("/linkout?remoteUrl=") {
            return Some(format!("https://www.curseforge.com{url}"));
        }
    }
    None
}

fn html_description(html: &str, curse: bool) -> Vec<DescriptionBlock> {
    use std::sync::LazyLock;
    static OMIT: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?is)<!--.*?-->|<(?:script|style|iframe|object|video)\b[^>]*>.*?</(?:script|style|iframe|object|video)\s*>").unwrap()
    });
    static TOKENS: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"(?s)<(/?)([a-zA-Z][a-zA-Z0-9]*)([^>"']|"[^"]*"|'[^']*')*>|([^<]+)"#)
            .unwrap()
    });
    static ATTR: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"(?i)\s(href|src|alt)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap()
    });
    let mut body = Vec::new();
    let mut fragments: Vec<DescriptionText> = Vec::new();
    let mut heading = 0;
    let mut bold = 0_u8;
    let mut link = None;
    let flush =
        |body: &mut Vec<DescriptionBlock>, fragments: &mut Vec<DescriptionText>, heading| {
            if fragments.iter().any(|span| !span.text.trim().is_empty()) {
                body.push(DescriptionBlock::Line {
                    heading,
                    fragments: std::mem::take(fragments),
                });
            } else {
                fragments.clear();
            }
        };
    let html = OMIT.replace_all(html, "");
    for token in TOKENS.captures_iter(&html).take(6000) {
        if body.len() >= 200 {
            break;
        }
        if let Some(raw) = token.get(4) {
            let mut text = html_entities(raw.as_str()).replace(['\n', '\r', '\t'], " ");
            if fragments.is_empty() {
                text = text.trim_start().to_owned();
            }
            if !text.is_empty() {
                if let Some(last) = fragments
                    .last_mut()
                    .filter(|s| s.bold == (bold > 0) && s.link == link)
                {
                    last.text.push_str(&text);
                } else {
                    fragments.push(DescriptionText {
                        text,
                        bold: bold > 0,
                        link: link.clone(),
                    });
                }
            }
            continue;
        }
        let name = token[2].to_ascii_lowercase();
        let close = &token[1] == "/";
        let tag = token.get(0).unwrap().as_str();
        let attr = |key: &str| {
            ATTR.captures_iter(tag).find_map(|cap| {
                cap[1]
                    .eq_ignore_ascii_case(key)
                    .then(|| cap.get(2).or_else(|| cap.get(3)).unwrap().as_str())
            })
        };
        match name.as_str() {
            "b" | "strong" => {
                bold = if close {
                    bold.saturating_sub(1)
                } else {
                    bold.saturating_add(1)
                }
            }
            "a" => {
                link = if close {
                    None
                } else {
                    attr("href").and_then(|url| description_url(url, curse))
                }
            }
            "img" if !close => {
                if let Some(url) = attr("src").and_then(|url| description_url(url, false)) {
                    flush(&mut body, &mut fragments, heading);
                    body.push(DescriptionBlock::Image {
                        url,
                        alt: html_entities(attr("alt").unwrap_or("")),
                        link: link.clone(),
                        heading,
                    });
                }
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                flush(&mut body, &mut fragments, heading);
                heading = if close { 0 } else { name.as_bytes()[1] - b'0' };
            }
            "p" | "div" | "li" | "br" | "ul" | "ol" | "blockquote" => {
                flush(&mut body, &mut fragments, heading);
                if name == "li" && !close {
                    fragments.push(DescriptionText {
                        text: "• ".into(),
                        bold: false,
                        link: None,
                    });
                }
            }
            _ => {}
        }
    }
    flush(&mut body, &mut fragments, heading);
    body
}

fn release_list(
    data: &Value,
    provider: Provider,
    mc: &str,
    curse_loaders: &[(u64, String, String)],
) -> Vec<Release> {
    let curse = provider == Provider::CurseForge;
    let is_stable = |v: &Value| {
        if curse {
            v["releaseType"].as_u64() == Some(1)
        } else {
            s(v, "version_type") == "release"
        }
    };
    items(data)
        .filter(|v| is_stable(v))
        .chain(items(data).filter(|v| !is_stable(v)))
        .filter_map(|v| {
            let id = if curse {
                v["id"].as_u64()?.to_string()
            } else {
                v["id"].as_str()?.into()
            };
            let game_versions = v.get(if curse {
                "gameVersions"
            } else {
                "game_versions"
            })?;
            let minecraft = items(game_versions).filter_map(Value::as_str).find(|v| {
                if mc.is_empty() {
                    v.starts_with(|c: char| c.is_ascii_digit())
                } else {
                    *v == mc
                }
            })?;
            let loader = items(if curse { game_versions } else { &v["loaders"] })
                .filter_map(Value::as_str)
                .find_map(|v| engine::loader_uid_and_name(v).map(|(_, _, name)| name))
                .or_else(|| {
                    curse_loaders
                        .iter()
                        .find(|(id, _, _)| Some(*id) == v["id"].as_u64())
                        .or_else(|| curse_loaders.iter().find(|(_, game, _)| game == minecraft))
                        .map(|(_, _, loader)| loader.as_str())
                })
                .unwrap_or(if curse { "Forge" } else { "Vanilla" });
            let (title, fallback) = if curse {
                (s(v, "displayName"), s(v, "fileName"))
            } else {
                (s(v, "name"), s(v, "version_number"))
            };
            if curse && !fallback.to_ascii_lowercase().ends_with(".zip") {
                return None;
            }
            if !curse && !items(&v["files"]).any(|f| s(f, "filename").ends_with(".mrpack")) {
                return None;
            }
            Some(Release {
                id,
                title: if title.is_empty() { fallback } else { title }.into(),
                minecraft: minecraft.into(),
                loader: loader.into(),
            })
        })
        .take(30)
        .collect()
}

pub fn details(project: &Project, mc: &str) -> Details {
    let mut result = Details {
        description: project.description.clone(),
        gallery: project.gallery.clone(),
        gallery_thumbnails: HashMap::new(),
        body: Vec::new(),
        versions: Ok(Vec::new()),
    };
    let id = encode(&project.id);
    match project.provider {
        Provider::CurseForge => {
            result.versions = cf_key().and_then(|key| {
                let client = cf_agent();
                let filter = if mc.is_empty() {
                    String::new()
                } else {
                    format!("&gameVersion={}", encode(mc))
                };
                let (files, desc, info) = std::thread::scope(|scope| {
                    let desc =
                        scope.spawn(|| cf_json(&client, &key, &format!("mods/{id}/description")));
                    let info = scope.spawn(|| cf_json(&client, &key, &format!("mods/{id}")));
                    let files = cf_json(
                        &client,
                        &key,
                        &format!("mods/{id}/files?pageSize=50{filter}"),
                    );
                    (
                        files,
                        desc.join().ok().and_then(Result::ok),
                        info.join().ok().and_then(Result::ok),
                    )
                });
                if let Some(html) = desc.as_ref().and_then(|v| v["data"].as_str()) {
                    result.body = html_description(html, true);
                }
                if let Some(info) = info {
                    let screenshots = &info["data"]["screenshots"];
                    let shots = gallery(screenshots);
                    if !shots.is_empty() {
                        result.gallery = shots;
                    }
                    result.gallery_thumbnails = items(screenshots)
                        .filter_map(|shot| {
                            let (url, thumb) = (s(shot, "url"), s(shot, "thumbnailUrl"));
                            (https(url) && https(thumb)).then(|| (url.into(), thumb.into()))
                        })
                        .collect();
                }
                files
                    .map(|v| release_list(&v["data"], project.provider, mc, &project.curse_loaders))
            });
        }
        Provider::Modrinth => {
            let client = agent();
            let filter = if mc.is_empty() {
                String::new()
            } else {
                format!("?game_versions={}", encode(&json!([mc]).to_string()))
            };
            let (versions, info) = std::thread::scope(|scope| {
                let info =
                    scope.spawn(|| json(client.get(&format!("{MODRINTH}/project/{id}")).call()));
                let versions = json(
                    client
                        .get(&format!("{MODRINTH}/project/{id}/version{filter}"))
                        .call(),
                );
                (versions, info.join().ok().and_then(Result::ok))
            });
            result.versions = versions.map(|v| release_list(&v, project.provider, mc, &[]));
            if let Some(info) = info {
                let shots = gallery(&info["gallery"]);
                if !shots.is_empty() {
                    result.gallery = shots;
                }
                let body = s(&info, "body");
                if !body.is_empty() {
                    result.description = body.into();
                }
            }
        }
    }
    if result.gallery.is_empty() && https(&project.icon) {
        result.gallery.push(project.icon.clone());
    }
    result
}

pub fn artwork(url: &str, size: u32) -> Option<image::RgbaImage> {
    artwork_with(&agent(), url, size)
}

fn artwork_with(client: &ureq::Agent, url: &str, size: u32) -> Option<image::RgbaImage> {
    if !https(url) {
        return None;
    }
    const MAX: u64 = 8 * 1024 * 1024;
    let mut bytes = Vec::new();
    client
        .get(url)
        .call()
        .ok()?
        .into_reader()
        .take(MAX + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX {
        return None;
    }
    let mut reader = image::ImageReader::new(io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    Some(reader.decode().ok()?.thumbnail(size, size).to_rgba8())
}

pub fn thumbnails(entries: &[(String, String)]) -> Vec<(String, image::RgbaImage)> {
    let sizes = entries
        .iter()
        .map(|(id, url)| (id.clone(), url.clone(), 160))
        .collect::<Vec<_>>();
    detail_images(&sizes)
}

pub fn detail_images(entries: &[(String, String, u32)]) -> Vec<(String, image::RgbaImage)> {
    if entries.is_empty() {
        return Vec::new();
    }
    let client = ureq::AgentBuilder::new()
        .user_agent(concat!("IkaLauncher/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(4))
        .build();
    std::thread::scope(|scope| {
        let workers = (0..5)
            .map(|offset| {
                let client = &client;
                scope.spawn(move || {
                    entries
                        .iter()
                        .skip(offset)
                        .step_by(5)
                        .filter_map(|(id, url, size)| {
                            Some((id.clone(), artwork_with(client, url, *size)?))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .filter_map(|worker| worker.join().ok())
            .flatten()
            .collect()
    })
}

pub(super) struct Installation {
    pub stage: tempfile::TempDir,
    pub minecraft: String,
    pub loader: String,
    pub loader_version: String,
    pub cfg: [(&'static str, String); 6],
}

fn safe_path(raw: &str) -> Result<PathBuf, String> {
    let mut out = PathBuf::new();
    for part in raw.split(['/', '\\']) {
        let base = part
            .split('.')
            .next()
            .unwrap_or("")
            .trim_end()
            .to_ascii_uppercase();
        let reserved = matches!(
            base.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        ) || base
            .strip_prefix("COM")
            .or_else(|| base.strip_prefix("LPT"))
            .is_some_and(|n| {
                [
                    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "¹", "²", "³",
                ]
                .contains(&n)
            });
        if part.is_empty()
            || part.ends_with(['.', ' '])
            || reserved
            || part
                .chars()
                .any(|c| c.is_control() || "<>:\"|?*".contains(c))
        {
            return Err("El modpack contiene una ruta no válida.".into());
        }
        out.push(part);
    }
    Ok(out)
}

struct Task {
    path: PathBuf,
    urls: Vec<String>,
    sha1: String,
    sha512: String,
    size: Option<u64>,
}

const MAX_FILE: u64 = 2 * 1024 * 1024 * 1024;

fn download(client: &ureq::Agent, task: &Task) -> Result<File, String> {
    let parent = task.path.parent().ok_or("Ruta de descarga no válida.")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut last = "Sin enlace de descarga.".to_owned();
    for url in task.urls.iter().filter(|u| https(u)) {
        let attempt = (|| {
            let mut response = client
                .get(url)
                .call()
                .map_err(|e| e.to_string())?
                .into_reader();
            let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
            let mut sha1 = Sha1::new();
            let mut sha512 = Sha512::new();
            let mut buffer = [0; 32768];
            let max = task.size.unwrap_or(MAX_FILE).min(MAX_FILE);
            let mut written = 0;
            loop {
                let n = response.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                written += n as u64;
                if written > max {
                    return Err("Tamaño de descarga incorrecto.".into());
                }
                let data = &buffer[..n];
                if !task.sha1.is_empty() {
                    sha1.update(data);
                }
                if !task.sha512.is_empty() {
                    sha512.update(data);
                }
                tmp.write_all(data).map_err(|e| e.to_string())?;
            }
            if written == 0
                || task.size.is_some_and(|size| written != size)
                || (!task.sha1.is_empty()
                    && !format!("{:x}", sha1.finalize()).eq_ignore_ascii_case(&task.sha1))
                || (!task.sha512.is_empty()
                    && !format!("{:x}", sha512.finalize()).eq_ignore_ascii_case(&task.sha512))
            {
                return Err("El archivo no coincide con su manifiesto.".into());
            }
            tmp.persist(&task.path).map_err(|e| e.error.to_string())
        })();
        match attempt {
            Ok(file) => return Ok(file),
            Err(e) => last = e,
        }
    }
    Err(format!(
        "{}: {last}",
        task.path.file_name().unwrap_or_default().to_string_lossy()
    ))
}

fn cf_urls(file: &Value) -> Vec<String> {
    let mut urls = Vec::new();
    let url = s(file, "downloadUrl");
    if https(url) {
        urls.push(url.into());
    }
    if let Some(id) = file["id"].as_u64() {
        let name = encode(s(file, "fileName"));
        for host in ["edge.forgecdn.net", "mediafilez.forgecdn.net"] {
            urls.push(format!(
                "https://{host}/files/{}/{}/{name}",
                id / 1000,
                id % 1000
            ));
        }
    }
    urls
}

fn cf_hash(file: &Value) -> String {
    items(&file["hashes"])
        .find(|h| h["algo"] == 1)
        .map(|h| s(h, "value"))
        .unwrap_or_default()
        .into()
}

fn cf_files(
    client: &ureq::Agent,
    key: &str,
    manifest: &Value,
    game: &Path,
) -> Result<Vec<Task>, String> {
    let mut requested = Vec::new();
    for file in items(&manifest["files"]).filter(|f| f["required"] != false) {
        let id = file["fileID"]
            .as_u64()
            .ok_or("El manifiesto tiene un archivo sin identificador.")?;
        let project = file["projectID"]
            .as_u64()
            .ok_or("El manifiesto tiene un archivo sin proyecto.")?;
        requested.push((id, project));
    }
    let mut tasks = Vec::with_capacity(requested.len());
    for chunk in requested.chunks(250) {
        let ids = chunk.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        let results = json(
            client
                .post(&format!("{}/mods/files", *CURSEFORGE))
                .set("x-api-key", key)
                .set("Content-Type", "application/json")
                .send_string(&json!({"fileIds": ids}).to_string()),
        )?;
        let files: HashMap<_, _> = items(&results["data"])
            .filter_map(|f| Some((f["id"].as_u64()?, f)))
            .collect();
        for (id, project) in chunk {
            let fetched;
            let file = if let Some(f) = files.get(id) {
                *f
            } else {
                fetched = cf_json(client, key, &format!("mods/{project}/files/{id}"))?;
                &fetched["data"]
            };
            let name = safe_path(s(file, "fileName"))?;
            if name.components().count() != 1
                || file["modId"]
                    .as_u64()
                    .is_some_and(|actual| actual != *project)
            {
                return Err(format!("Metadatos no válidos para el archivo {id}."));
            }
            let folder = if s(file, "fileName").to_ascii_lowercase().ends_with(".zip") {
                if items(&file["modules"]).any(|m| {
                    matches!(
                        s(m, "name").to_ascii_lowercase().as_str(),
                        "shader" | "shaders"
                    )
                }) {
                    "shaderpacks"
                } else {
                    "resourcepacks"
                }
            } else {
                "mods"
            };
            tasks.push(Task {
                path: game.join(folder).join(name),
                urls: cf_urls(file),
                sha1: cf_hash(file),
                sha512: String::new(),
                size: file["fileLength"].as_u64().filter(|n| *n > 0),
            });
        }
    }
    Ok(tasks)
}

fn modrinth_files(manifest: &Value, game: &Path) -> Result<Vec<Task>, String> {
    items(&manifest["files"])
        .filter(|f| f["env"]["client"] != "unsupported")
        .map(|file| {
            let path = safe_path(s(file, "path"))?;
            let urls = items(&file["downloads"])
                .filter_map(Value::as_str)
                .filter(|u| https(u))
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let sha1 = s(&file["hashes"], "sha1");
            let sha512 = s(&file["hashes"], "sha512");
            if urls.is_empty() || (sha1.is_empty() && sha512.is_empty()) {
                return Err(
                    "El manifiesto de Modrinth no incluye la descarga o su checksum.".into(),
                );
            }
            Ok(Task {
                path: game.join(path),
                urls,
                sha1: sha1.into(),
                sha512: sha512.into(),
                size: file["fileSize"].as_u64(),
            })
        })
        .collect()
}

fn extract_overrides(
    archive: &mut zip::ZipArchive<File>,
    game: &Path,
    prefixes: &[&str],
) -> Result<(), String> {
    let mut remaining = 8 * 1024 * 1024 * 1024_u64;
    for prefix in prefixes {
        let prefix = format!("{}/", prefix.trim_end_matches('/'));
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().replace('\\', "/");
            let Some(raw) = name.strip_prefix(&prefix) else {
                continue;
            };
            let dest = game.join(safe_path(raw)?);
            let declared = entry.size();
            if declared > remaining || declared > MAX_FILE {
                return Err("El contenido del modpack es demasiado grande.".into());
            }
            fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
            let mut file = tempfile::NamedTempFile::new_in(dest.parent().unwrap())
                .map_err(|e| e.to_string())?;
            let count = io::copy(&mut (&mut entry).take(declared + 1), &mut file)
                .map_err(|e| e.to_string())?;
            if count != declared {
                return Err("Tamaño incorrecto en los archivos del modpack.".into());
            }
            remaining -= count;
            file.persist(dest).map_err(|e| e.error.to_string())?;
        }
    }
    Ok(())
}

fn metadata(manifest: &Value, provider: Provider) -> Result<(String, String, String), String> {
    let (mc, loader) = match provider {
        Provider::Modrinth => {
            let deps = &manifest["dependencies"];
            let loader = ["fabric-loader", "quilt-loader", "neoforge", "forge"]
                .iter()
                .find_map(|k| deps[*k].as_str().map(|v| (*k, v)));
            (s(deps, "minecraft"), loader)
        }
        Provider::CurseForge => {
            let game = &manifest["minecraft"];
            let loaders = &game["modLoaders"];
            let id = items(loaders)
                .find(|v| v["primary"] == true)
                .or_else(|| items(loaders).next())
                .map(|l| s(l, "id"));
            (s(game, "version"), id.and_then(|id| id.split_once('-')))
        }
    };
    if !version_token(mc) {
        return Err("El manifiesto no indica una versión válida de Minecraft.".into());
    }
    let (loader, version) = if let Some((name, version)) = loader {
        let (_, _, loader) =
            engine::loader_uid_and_name(name).ok_or("El modpack usa un loader desconocido.")?;
        let prefix = format!("{mc}-");
        let version = version.strip_prefix(&prefix).unwrap_or(version);
        if !version_token(version) {
            return Err("El manifiesto no indica una versión válida del loader.".into());
        }
        (loader, version)
    } else {
        ("Vanilla", "")
    };
    Ok((mc.into(), loader.into(), version.into()))
}

pub(super) fn install(
    project: &Project,
    release: &Release,
    progress: &(impl Fn(String) + Sync),
) -> Result<Installation, String> {
    let root = engine::instances_root();
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let stage = tempfile::Builder::new()
        .prefix(".ika-creating-")
        .tempdir_in(root)
        .map_err(|e| e.to_string())?;
    let game = stage.path().join(".minecraft");
    let client = ureq::AgentBuilder::new()
        .user_agent(concat!("IkaLauncher/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(180))
        .build();
    let path = stage.path().join("pack-download.zip");
    progress("Descargando el modpack…".into());
    let cf = if project.provider == Provider::CurseForge {
        Some((cf_agent(), cf_key()?))
    } else {
        None
    };
    let task = if let Some((c, key)) = &cf {
        let info = cf_json(
            c,
            key,
            &format!("mods/{}/files/{}", encode(&project.id), encode(&release.id)),
        )?;
        let file = &info["data"];
        Task {
            path: path.clone(),
            urls: cf_urls(file),
            sha1: cf_hash(file),
            sha512: String::new(),
            size: file["fileLength"].as_u64().filter(|n| *n > 0),
        }
    } else {
        let url = format!("{MODRINTH}/version/{}", encode(&release.id));
        let info = json(agent().get(&url).call())?;
        if info["project_id"] != project.id {
            return Err("La versión no pertenece al modpack seleccionado.".into());
        }
        let valid = || items(&info["files"]).filter(|f| s(f, "filename").ends_with(".mrpack"));
        let file = valid()
            .find(|f| f["primary"] == true)
            .or_else(|| valid().next())
            .ok_or("No se encontró el archivo .mrpack.")?;
        Task {
            path: path.clone(),
            urls: vec![s(file, "url").into()],
            sha1: s(&file["hashes"], "sha1").into(),
            sha512: s(&file["hashes"], "sha512").into(),
            size: file["size"].as_u64(),
        }
    };
    let file = download(&client, &task)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Archivo de modpack no válido: {e}"))?;
    let manifest: Value = {
        let name = if cf.is_some() {
            "manifest.json"
        } else {
            "modrinth.index.json"
        };
        let entry = archive
            .by_name(name)
            .map_err(|_| format!("Falta {name} en el modpack."))?;
        if entry.size() > 16 * 1024 * 1024 {
            return Err("El manifiesto del modpack es demasiado grande.".into());
        }
        serde_json::from_reader(entry.take(16 * 1024 * 1024))
            .map_err(|e| format!("Manifiesto del modpack no válido: {e}"))?
    };
    let (minecraft, loader, loader_version) = metadata(&manifest, project.provider)?;
    if !manifest["files"].is_array() {
        return Err("Falta la lista de archivos del modpack.".into());
    }
    progress("Preparando archivos del modpack…".into());
    let mut tasks = if let Some((c, key)) = &cf {
        cf_files(c, key, &manifest, &game)?
    } else {
        modrinth_files(&manifest, &game)?
    };
    let mut seen = std::collections::HashSet::new();
    if tasks
        .iter()
        .any(|task| !seen.insert(task.path.to_string_lossy().to_lowercase()))
    {
        return Err("El manifiesto contiene rutas de descarga duplicadas.".into());
    }
    let total = tasks.len();
    let count = std::sync::atomic::AtomicUsize::new(0);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(6)
        .build()
        .map_err(|e| e.to_string())?;
    use rayon::prelude::*;
    pool.install(|| {
        tasks.par_iter_mut().try_for_each(|item| {
            download(&client, item)?;
            let done = count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if done % 12 == 0 || done == total {
                progress(format!("Descargando archivos ({done}/{total})…"));
            }
            Ok::<_, String>(())
        })
    })?;
    progress("Aplicando configuración del modpack…".into());
    let overrides = if cf.is_some() {
        manifest["overrides"].as_str().unwrap_or("overrides")
    } else {
        "overrides"
    };
    extract_overrides(&mut archive, &game, &[overrides, "client-overrides"])?;
    drop(archive);
    fs::remove_file(path).map_err(|e| e.to_string())?;
    Ok(Installation {
        stage,
        minecraft,
        loader,
        loader_version,
        cfg: [
            ("ManagedPack", "true".into()),
            (
                "ManagedPackType",
                project.provider.label().to_ascii_lowercase(),
            ),
            ("ManagedPackID", project.id.clone()),
            ("ManagedPackName", project.title.clone()),
            ("ManagedPackVersionID", release.id.clone()),
            ("ManagedPackVersionName", release.title.clone()),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_description_keeps_the_header_art_and_safe_links() {
        let body = html_description(
            r#"<h5><strong><img src="https://cdn.example/features.png"></strong></h5>
               <h5><strong>✔️ 100+ Quests &amp; <a href="https://example.org/wiki">Wiki</a></strong></h5>
               <script>ignored()</script><p><a href="javascript:ignored()">Plain</a>
               <a href="/linkout?remoteUrl=https%253a%252f%252fexample.org">External</a></p>"#,
            true,
        );
        assert!(
            matches!(&body[0], DescriptionBlock::Image { url, heading: 5, .. } if url.ends_with("/features.png"))
        );
        let DescriptionBlock::Line {
            heading: 5,
            fragments,
        } = &body[1]
        else {
            panic!("missing formatted feature line");
        };
        assert!(fragments[0].bold && fragments[0].text.contains("100+ Quests & "));
        assert_eq!(
            fragments[1].link.as_deref(),
            Some("https://example.org/wiki")
        );
        let DescriptionBlock::Line { fragments, .. } = &body[2] else {
            panic!("missing final paragraph");
        };
        assert!(fragments[0].link.is_none() && fragments[0].text.trim() == "Plain");
        assert!(
            fragments[1]
                .link
                .as_ref()
                .is_some_and(|url| url.starts_with("https://www.curseforge.com/linkout?"))
        );
        assert_eq!(body.len(), 3);
    }

    #[test]
    fn pack_versions_match_filter_and_manifest_preserves_exact_loader() {
        let versions = json!([
            {"id":"wrong-mc","name":"New","game_versions":["1.21.5"],"loaders":["fabric"],"files":[{"filename":"new.mrpack"}]},
            {"id":"not-a-pack","name":"Other","game_versions":["1.21.4"],"loaders":["fabric"],"files":[{"filename":"other.zip"}]},
            {"id":"selected","name":"Selected","game_versions":["1.21.4"],"loaders":["neoforge"],"files":[{"filename":"test.mrpack"}]}
        ]);
        let list = release_list(&versions, Provider::Modrinth, "1.21.4", &[]);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "selected");
        assert_eq!(list[0].loader, "NeoForge");
        let manifest = json!({"minecraft":{"version":"1.20.1","modLoaders":[
            {"id":"forge-1.20.1-47.4.0","primary":true}
        ]}});
        assert_eq!(
            metadata(&manifest, Provider::CurseForge).unwrap(),
            ("1.20.1".into(), "Forge".into(), "47.4.0".into())
        );
    }

    #[test]
    fn pack_paths_cannot_escape_the_staged_game_folder_on_windows() {
        for path in [
            "../x",
            "mods/../x",
            "C:/x",
            "\\x",
            "mods/x:ads",
            "mods/NUL.txt",
            "mods/x.",
            "mods/x ",
        ] {
            assert!(safe_path(path).is_err(), "accepted {path}");
        }
        assert_eq!(
            safe_path("config/options.txt").unwrap(),
            Path::new("config").join("options.txt")
        );
    }
}
