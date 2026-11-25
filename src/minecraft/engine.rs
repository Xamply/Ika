//! Standalone Minecraft installer + offline launcher.
//!
//! Works like Prism Launcher / PineconeMC without needing either installed: instances use
//! Prism's format (`instance.cfg` + `mmc-pack.json`), components are resolved from the
//! public Prism meta (`meta.prismlauncher.org`, which also has Forge/NeoForge ready to run
//! through ForgeWrapper), every file is downloaded and SHA-1 checked into the launcher's own
//! data dir (`%APPDATA%\IkaLauncher`), and the game is started directly with an offline
//! account (Prism's name-based UUID).

use super::java;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const META_URL: &str = "https://meta.prismlauncher.org/v1";
const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net/";
const RESOURCES_URL: &str = "https://resources.download.minecraft.net";
const USER_AGENT: &str = "IkaLauncher/1.0";
const FORGEWRAPPER_MAIN: &str = "io.github.zekerzhayard.forgewrapper.installer.Main";
const DOWNLOAD_THREADS: usize = 16;
/// Launcher data dir: instances, libraries, assets, meta cache and Java runtimes.
pub fn ika_root() -> PathBuf {
    let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(appdata).join("IkaLauncher")
}

pub fn instances_root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .and_then(|root| std::fs::read_to_string(PathBuf::from(root).join("IkaNative/instance-folder.txt")).ok())
        .map(|value| PathBuf::from(value.trim()))
        .filter(|path| path.is_absolute() && path.is_dir())
        .unwrap_or_else(|| ika_root().join("instances"))
}

struct Progress<'a>(&'a (dyn Fn(&str, usize, usize) + Sync));

impl Progress<'_> {
    fn emit(&self, _stage: &str, message: &str, done: usize, total: usize) {
        (self.0)(message, done, total);
    }
    fn emit_with_speed(
        &self,
        stage: &str,
        message: &str,
        done: usize,
        total: usize,
        _speed: Option<&str>,
    ) {
        self.emit(stage, message, done, total);
    }
}
fn format_download_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} GB/s", bytes_per_sec / (1024.0 * 1024.0 * 1024.0))
    } else if bytes_per_sec >= 1024.0 * 1024.0 {
        format!("{:.1} MB/s", bytes_per_sec / (1024.0 * 1024.0))
    } else if bytes_per_sec >= 1024.0 {
        format!("{:.0} KB/s", bytes_per_sec / 1024.0)
    } else if bytes_per_sec > 0.0 {
        format!("{:.0} B/s", bytes_per_sec)
    } else {
        String::new()
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout_read(std::time::Duration::from_secs(60))
        .user_agent(USER_AGENT)
        .build()
}

fn http_get_bytes(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, String> {
    let resp = agent
        .get(url)
        .call()
        .map_err(|e| format!("{}: {}", url, e))?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf)
        .map_err(|e| format!("{}: {}", url, e))?;
    Ok(buf)
}

fn sha1_hex(data: &[u8]) -> String {
    use sha1::Digest;
    let mut h = sha1::Sha1::new();
    h.update(data);
    h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
}

fn sha1_file(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|d| sha1_hex(&d))
}

#[derive(Clone, Debug)]
struct Download {
    url: String,
    path: PathBuf,
    sha1: Option<String>,
    size: Option<u64>,
}

impl Download {
    /// Already on disk and (as far as we can tell cheaply) complete.
    fn satisfied(&self) -> bool {
        match std::fs::metadata(&self.path) {
            Ok(m) if m.is_file() => self.size.map_or(m.len() > 0, |s| s == m.len()),
            _ => false,
        }
    }

    fn fetch(&self, agent: &ureq::Agent) -> Result<usize, String> {
        let mut last_err = String::new();
        for attempt in 0..3 {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(600 * attempt));
            }
            match http_get_bytes(agent, &self.url) {
                Ok(data) => {
                    if let Some(expected) = &self.sha1 {
                        let got = sha1_hex(&data);
                        if !got.eq_ignore_ascii_case(expected) {
                            last_err = format!("{}: checksum incorrecto", self.url);
                            continue;
                        }
                    }
                    if let Some(parent) = self.path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    // Write to a temp file first so an interrupted write never looks complete.
                    let mut tmp = self.path.clone().into_os_string();
                    tmp.push(".part");
                    let tmp = PathBuf::from(tmp);
                    let len = data.len();
                    std::fs::write(&tmp, &data)
                        .map_err(|e| format!("{}: {}", self.path.display(), e))?;
                    std::fs::rename(&tmp, &self.path)
                        .map_err(|e| format!("{}: {}", self.path.display(), e))?;
                    return Ok(len);
                }
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }
}

/// Downloads every missing file in parallel, reporting progress under `stage`.
fn download_all(
    list: Vec<Download>,
    progress: &Progress,
    stage: &str,
    label: &str,
) -> Result<(), String> {
    use rayon::prelude::*;
    let mut seen = HashSet::with_capacity(list.len());
    let unique: Vec<Download> = list
        .into_iter()
        .filter(|d| seen.insert(d.path.clone()))
        .collect();
    let pending: Vec<Download> = if unique.len() > 32 {
        unique.into_par_iter().filter(|d| !d.satisfied()).collect()
    } else {
        unique.into_iter().filter(|d| !d.satisfied()).collect()
    };
    let total = pending.len();
    if total == 0 {
        return Ok(());
    }
    progress.emit(stage, label, 0, total);
    let done = AtomicUsize::new(0);
    let downloaded_bytes = AtomicUsize::new(0);
    let start_time = std::time::Instant::now();
    let last_emit = Mutex::new(std::time::Instant::now());
    let errors = Mutex::new(Vec::<String>::new());
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(DOWNLOAD_THREADS)
        .build()
        .map_err(|e| e.to_string())?;
    pool.install(|| {
        pending.par_iter().for_each_init(agent, |agent, d| {
            match d.fetch(agent) {
                Ok(bytes) => {
                    downloaded_bytes.fetch_add(bytes, Ordering::Relaxed);
                }
                Err(e) => {
                    errors.lock().unwrap_or_else(|e| e.into_inner()).push(e);
                }
            }
            let n = done.fetch_add(1, Ordering::SeqCst) + 1;
            let mut last = last_emit.lock().unwrap_or_else(|e| e.into_inner());
            if n == total || last.elapsed().as_millis() > 120 {
                *last = std::time::Instant::now();
                let elapsed = start_time.elapsed().as_secs_f64();
                let bytes = downloaded_bytes.load(Ordering::Relaxed);
                let speed_str = if elapsed > 0.25 && bytes > 0 {
                    format_download_speed((bytes as f64) / elapsed)
                } else {
                    String::new()
                };
                progress.emit_with_speed(
                    stage,
                    label,
                    n,
                    total,
                    if speed_str.is_empty() {
                        None
                    } else {
                        Some(&speed_str)
                    },
                );
            }
        });
    });
    let errors = errors.into_inner().unwrap_or_default();
    if !errors.is_empty() {
        return Err(format!(
            "No se pudieron descargar {} archivo(s). Revisa tu conexión.\n{}",
            errors.len(),
            errors
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Meta
// ---------------------------------------------------------------------------------------

/// Version file of a component. Versions are immutable, so a cached copy is used first and
/// the network is only needed the first time (this is what makes later launches offline).
fn load_meta(agent: &ureq::Agent, uid: &str, version: &str) -> Result<Value, String> {
    let cache = ika_root()
        .join("meta")
        .join(uid)
        .join(format!("{}.json", version));
    if let Ok(text) = std::fs::read_to_string(&cache) {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            return Ok(v);
        }
    }
    let url = format!("{}/{}/{}.json", META_URL, uid, version);
    let data = http_get_bytes(agent, &url).map_err(|e| {
        format!(
            "No se pudo obtener {} {} (¿sin conexión?): {}",
            uid, version, e
        )
    })?;
    let v: Value = serde_json::from_slice(&data)
        .map_err(|e| format!("Metadatos inválidos de {}: {}", uid, e))?;
    if let Some(parent) = cache.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&cache, &data);
    Ok(v)
}

/// Mojang's official version file for `mc_version` (cached like the Prism meta).
fn load_mojang_version(agent: &ureq::Agent, mc_version: &str) -> Result<Value, String> {
    let cache = ika_root()
        .join("meta")
        .join("mojang")
        .join(format!("{}.json", mc_version));
    if let Ok(v) = std::fs::read_to_string(&cache)
        .map_err(|_| ())
        .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|_| ()))
    {
        return Ok(v);
    }
    let manifest: Value = serde_json::from_slice(&http_get_bytes(
        agent,
        "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
    )?)
    .map_err(|e| e.to_string())?;
    let url = manifest
        .get("versions")
        .and_then(|v| v.as_array())
        .and_then(|vs| {
            vs.iter()
                .find(|v| v.get("id").and_then(|i| i.as_str()) == Some(mc_version))
        })
        .and_then(|v| v.get("url").and_then(|u| u.as_str()))
        .ok_or_else(|| format!("Mojang no tiene la versión {}", mc_version))?
        .to_string();
    let data = http_get_bytes(agent, &url)?;
    let v: Value = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    if let Some(parent) = cache.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&cache, &data);
    Ok(v)
}

#[derive(Clone, Debug)]
struct Component {
    uid: String,
    version: String,
}

/// Reads the components of an instance and adds the ones they require (LWJGL, Intermediary...),
/// the same way Prism's PackProfile resolves them. Returns them with their version files,
/// sorted by the meta `order` field (the order Prism applies them in).
fn resolve_components(
    agent: &ureq::Agent,
    inst_dir: &Path,
) -> Result<Vec<(Component, Value)>, String> {
    let pack_text = std::fs::read_to_string(inst_dir.join("mmc-pack.json"))
        .map_err(|_| "La instancia no tiene mmc-pack.json".to_string())?;
    let pack: Value =
        serde_json::from_str(&pack_text).map_err(|e| format!("mmc-pack.json inválido: {}", e))?;

    let mut comps: Vec<Component> = Vec::new();
    for c in pack
        .get("components")
        .and_then(|c| c.as_array())
        .into_iter()
        .flatten()
    {
        let uid = c
            .get("uid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let version = c
            .get("version")
            .and_then(|v| v.as_str())
            .or_else(|| c.get("cachedVersion").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        if !uid.is_empty() {
            comps.push(Component { uid, version });
        }
    }

    let mc_version = comps
        .iter()
        .find(|c| c.uid == "net.minecraft")
        .map(|c| c.version.clone())
        .ok_or("La instancia no tiene versión de Minecraft")?;

    // Self-heal instances created without a loader component when IntendedLoader or mods exist.
    let has_loader_comp = comps.iter().any(|c| {
        matches!(
            c.uid.as_str(),
            "net.fabricmc.fabric-loader"
                | "org.quiltmc.quilt-loader"
                | "net.minecraftforge"
                | "net.neoforged"
        )
    });
    if !has_loader_comp {
        let cfg = read_cfg(inst_dir);
        let intended = cfg
            .get("IntendedLoader")
            .cloned()
            .or_else(|| detect_loader_from_mods(&game_dir(inst_dir)));
        if let Some((uid, _, _)) = intended.as_deref().and_then(loader_uid_and_name) {
            if let Some(ver) = resolve_loader_uid_version(agent, uid, &mc_version, None) {
                comps.push(Component {
                    uid: uid.to_string(),
                    version: ver,
                });
            }
        }
    }

    let mut loaded: HashMap<String, Value> = HashMap::new();
    let mut i = 0;
    while i < comps.len() {
        let mut comp = comps[i].clone();
        // Custom patches (instance/patches/<uid>.json) take the place of the meta file.
        let patch = inst_dir.join("patches").join(format!("{}.json", comp.uid));
        let meta = if patch.exists() {
            let text = std::fs::read_to_string(&patch).map_err(|e| e.to_string())?;
            serde_json::from_str(&text)
                .map_err(|e| format!("Parche {} inválido: {}", comp.uid, e))?
        } else {
            if comp.version.is_empty() {
                if let Some(resolved) =
                    resolve_loader_uid_version(agent, &comp.uid, &mc_version, None)
                {
                    comp.version = resolved.clone();
                    comps[i].version = resolved;
                } else {
                    return Err(format!("El componente {} no tiene versión", comp.uid));
                }
            }
            load_meta(agent, &comp.uid, &comp.version)?
        };
        for req in meta
            .get("requires")
            .and_then(|r| r.as_array())
            .into_iter()
            .flatten()
        {
            let ruid = req.get("uid").and_then(|v| v.as_str()).unwrap_or("");
            if ruid.is_empty() || comps.iter().any(|c| c.uid == ruid) {
                continue;
            }
            // LWJGL 2 and 3 conflict: don't add one when the pack already has the other.
            if (ruid == "org.lwjgl3" && comps.iter().any(|c| c.uid == "org.lwjgl"))
                || (ruid == "org.lwjgl" && comps.iter().any(|c| c.uid == "org.lwjgl3"))
            {
                continue;
            }
            let version = req
                .get("equals")
                .and_then(|v| v.as_str())
                .or_else(|| req.get("suggests").and_then(|v| v.as_str()))
                .map(str::to_string)
                .or_else(|| (ruid == "net.fabricmc.intermediary").then(|| mc_version.clone()))
                .ok_or_else(|| format!("No se pudo resolver la dependencia {}", ruid))?;
            comps.push(Component {
                uid: ruid.to_string(),
                version,
            });
        }
        loaded.insert(comp.uid.clone(), meta);
        i += 1;
    }

    let mut out: Vec<(Component, Value)> = comps
        .into_iter()
        .map(|c| {
            let m = loaded.remove(&c.uid).unwrap_or(Value::Null);
            (c, m)
        })
        .collect();
    out.sort_by_key(|(_, m)| m.get("order").and_then(|o| o.as_i64()).unwrap_or(0));
    Ok(out)
}

// ---------------------------------------------------------------------------------------
// Launch profile (Prism's LaunchProfile: the components merged in order)
// ---------------------------------------------------------------------------------------

#[derive(Default)]
struct Profile {
    mc_version: String,
    version_type: String,
    main_class: String,
    minecraft_arguments: String,
    tweakers: Vec<String>,
    jvm_args: Vec<String>,
    traits: HashSet<String>,
    libraries: Vec<Value>,
    maven_files: Vec<Value>,
    main_jar: Option<Value>,
    asset_index: Option<Value>,
    logging: Option<Value>,
    java_majors: Vec<u32>,
    java_name: Option<String>,
    /// Files in the instance's jarmods/ folder, in component order.
    jar_mods: Vec<String>,
}

/// group:artifact[:classifier] — a later component's library with the same key replaces it.
fn library_key(name: &str) -> String {
    let base = name.split('@').next().unwrap_or(name);
    let parts: Vec<&str> = base.split(':').collect();
    match parts.len() {
        0..=2 => base.to_string(),
        3 => format!("{}:{}", parts[0], parts[1]),
        _ => format!("{}:{}:{}", parts[0], parts[1], parts[3]),
    }
}

fn str_list(v: Option<&Value>) -> Vec<String> {
    v.and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

impl Profile {
    fn apply(&mut self, comp: &Component, m: &Value) {
        if comp.uid == "net.minecraft" {
            self.mc_version = comp.version.clone();
            self.version_type = m
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("release")
                .to_string();
        }
        if let Some(s) = m.get("mainClass").and_then(|v| v.as_str()) {
            self.main_class = s.to_string();
        }
        if let Some(s) = m.get("minecraftArguments").and_then(|v| v.as_str()) {
            self.minecraft_arguments = s.to_string();
        }
        self.tweakers.extend(str_list(m.get("+tweakers")));
        self.jvm_args.extend(str_list(m.get("+jvmArgs")));
        self.traits.extend(str_list(m.get("+traits")));
        // Like Prism's LaunchProfile::applyLibrary: libraries whose rules exclude Windows are
        // skipped first, and native libraries are matched only against native ones. A file
        // can list the same library several times (1.12.2 has text2speech plain, with
        // natives and macOS-only); replacing blindly kept the macOS one and dropped the
        // Windows ones, which crashed the game ("NoClassDefFoundError: Narrator").
        let slot = |l: &Value| {
            let name = l.get("name").and_then(|n| n.as_str()).unwrap_or("");
            (library_key(name), l.get("natives").is_some())
        };
        for key in ["libraries", "+libraries"] {
            for lib in m.get(key).and_then(|l| l.as_array()).into_iter().flatten() {
                if !rules_allow(lib) {
                    continue;
                }
                let k = slot(lib);
                if let Some(pos) = self.libraries.iter().position(|l| slot(l) == k) {
                    self.libraries[pos] = lib.clone();
                } else {
                    self.libraries.push(lib.clone());
                }
            }
        }
        for f in m
            .get("mavenFiles")
            .and_then(|l| l.as_array())
            .into_iter()
            .flatten()
        {
            self.maven_files.push(f.clone());
        }
        if let Some(j) = m.get("mainJar") {
            self.main_jar = Some(j.clone());
        }
        if let Some(a) = m.get("assetIndex") {
            self.asset_index = Some(a.clone());
        }
        if let Some(l) = m.get("logging") {
            self.logging = Some(l.clone());
        }
        if let Some(majors) = m.get("compatibleJavaMajors").and_then(|a| a.as_array()) {
            self.java_majors = majors
                .iter()
                .filter_map(|x| x.as_u64().map(|n| n as u32))
                .collect();
        }
        if let Some(n) = m.get("compatibleJavaName").and_then(|v| v.as_str()) {
            self.java_name = Some(n.to_string());
        }
        for jm in m
            .get("jarMods")
            .and_then(|j| j.as_array())
            .into_iter()
            .flatten()
        {
            // Prism: "MMC-filename", or the id in "custom.jarmods:<id>:1" + ".jar"
            let file = jm
                .get("MMC-filename")
                .and_then(|f| f.as_str())
                .map(str::to_string)
                .or_else(|| {
                    jm.get("name")
                        .and_then(|n| n.as_str())
                        .and_then(|n| n.split(':').nth(1))
                        .map(|id| format!("{}.jar", id))
                });
            if let Some(f) = file {
                self.jar_mods.push(f);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Libraries
// ---------------------------------------------------------------------------------------

/// Maven path of "group:artifact:version[:classifier][@ext]".
fn maven_path(name: &str, classifier_override: Option<&str>) -> Option<String> {
    let (coords, ext) = match name.split_once('@') {
        Some((c, e)) => (c, e),
        None => (name, "jar"),
    };
    let parts: Vec<&str> = coords.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    let classifier = classifier_override.or_else(|| parts.get(3).copied());
    let file = match classifier {
        Some(c) if !c.is_empty() => format!("{}-{}-{}.{}", artifact, version, c, ext),
        _ => format!("{}-{}.{}", artifact, version, ext),
    };
    Some(format!(
        "{}/{}/{}/{}",
        group.replace('.', "/"),
        artifact,
        version,
        file
    ))
}

/// Mojang/Prism library rules, evaluated for Windows x64.
fn rules_allow(lib: &Value) -> bool {
    let Some(rules) = lib.get("rules").and_then(|r| r.as_array()) else {
        return true;
    };
    let mut allowed = false;
    for rule in rules {
        let applies = match rule.get("os") {
            None => true,
            Some(os) => {
                let name_ok = os
                    .get("name")
                    .and_then(|n| n.as_str())
                    .map_or(true, |n| n == "windows");
                let arch_ok = os
                    .get("arch")
                    .and_then(|a| a.as_str())
                    .map_or(true, |a| a == "x86_64" || a == "x64");
                name_ok && arch_ok
            }
        };
        if applies {
            allowed = rule.get("action").and_then(|a| a.as_str()) == Some("allow");
        }
    }
    allowed
}

/// Where a library file lives and how to get it. `classifier` selects a native classifier.
fn library_download(lib: &Value, libs_dir: &Path, classifier: Option<&str>) -> Option<Download> {
    let name = lib.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let artifact = match classifier {
        Some(c) => lib
            .pointer("/downloads/classifiers")
            .and_then(|cl| cl.get(c)),
        None => lib.pointer("/downloads/artifact"),
    };
    let rel = artifact
        .and_then(|a| a.get("path").and_then(|p| p.as_str()).map(str::to_string))
        .or_else(|| maven_path(name, classifier))?;
    let path = libs_dir.join(rel.replace('/', "\\"));
    let url = match artifact.and_then(|a| a.get("url").and_then(|u| u.as_str())) {
        Some(u) => u.to_string(),
        None => {
            let base = lib
                .get("url")
                .and_then(|u| u.as_str())
                .unwrap_or(MOJANG_LIBRARIES);
            format!("{}/{}", base.trim_end_matches('/'), rel)
        }
    };
    Some(Download {
        url,
        path,
        sha1: artifact.and_then(|a| a.get("sha1").and_then(|s| s.as_str()).map(str::to_string)),
        size: artifact.and_then(|a| a.get("size").and_then(|s| s.as_u64())),
    })
}

/// Old-style native classifier for Windows ("natives-windows", "natives-windows-${arch}").
fn native_classifier(lib: &Value) -> Option<String> {
    lib.pointer("/natives/windows")
        .and_then(|n| n.as_str())
        .map(|c| c.replace("${arch}", "64"))
}

// ---------------------------------------------------------------------------------------
// Instance files
// ---------------------------------------------------------------------------------------

pub(super) fn read_cfg(inst_dir: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(text) = std::fs::read_to_string(inst_dir.join("instance.cfg")) {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    map
}

/// Game root: ".minecraft" or "minecraft" if present, Battly root if instance.json/mods in root, else ".minecraft".
pub(super) fn game_dir(inst_dir: &Path) -> PathBuf {
    let dot = inst_dir.join(".minecraft");
    let plain = inst_dir.join("minecraft");
    if dot.is_dir() {
        dot
    } else if plain.is_dir() {
        plain
    } else if inst_dir.join("instance.json").is_file() || inst_dir.join("mods").is_dir() {
        inst_dir.to_path_buf()
    } else if inst_dir.join("cretania").is_dir() && inst_dir.join("versions").is_dir() {
        // Original Cretania/Lucerion layout: launcher root plus separate game dir.
        inst_dir.join("cretania")
    } else {
        dot
    }
}

/// Offline UUID, same as Prism/Pinecone and Java's `UUID.nameUUIDFromBytes`: the MD5 of
/// "OfflinePlayer:<name>" with the version-3 bits set.
fn offline_uuid(name: &str) -> String {
    use md5::Digest;
    let mut b: [u8; 16] = md5::Md5::digest(format!("OfflinePlayer:{}", name).as_bytes()).into();
    b[6] = (b[6] & 0x0f) | 0x30;
    b[8] = (b[8] & 0x3f) | 0x80;
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

/// Reverses Prism/QSettings INI quoting of a value (`"..."`, `\\`, `\"`).
pub(super) fn unescape_ini(v: &str) -> String {
    let v = v.trim();
    let v = v
        .strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .unwrap_or(v);
    let mut out = String::new();
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Splits user JVM arguments, keeping "quoted values" together.
fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for ch in s.chars() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"') | (None, '\'') => quote = Some(ch),
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn replace_tokens(s: &str, tokens: &HashMap<&str, String>) -> String {
    let mut out = s.to_string();
    for (k, v) in tokens {
        out = out.replace(&format!("${{{}}}", k), v);
    }
    out
}

fn java_major_of(path: &Path) -> Option<u32> {
    // <home>/bin/javaw.exe -> <home>/release
    let home = path.parent()?.parent()?;
    let text = std::fs::read_to_string(home.join("release")).ok()?;
    let line = text.lines().find(|l| l.starts_with("JAVA_VERSION="))?;
    let v = line["JAVA_VERSION=".len()..].trim().trim_matches('"');
    let mut parts = v.split(|c: char| c == '.' || c == '_' || c == '+' || c == '-');
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// SkinRestorer exists for Minecraft 1.19+ (and the new 26.x numbering).
fn set_cfg_values(inst_dir: &Path, values: &[(&str, String)]) {
    let path = inst_dir.join("instance.cfg");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    for (key, value) in values {
        let entry = format!("{}={}", key, value);
        match lines
            .iter()
            .position(|l| l.split_once('=').map_or(false, |(k, _)| k.trim() == *key))
        {
            Some(i) => lines[i] = entry,
            None => lines.push(entry),
        }
    }
    let _ = std::fs::write(&path, lines.join("\n") + "\n");
}

/// Java for the launch, in this order:
/// 1. the Java path saved in the instance, if it's still on disk;
/// 2. the exact runtime chosen when the instance was created (vendor + build): reused if
///    it's already installed, otherwise downloaded now and saved into the instance;
/// 3. no choice made: an installed Java compatible with this Minecraft version, or
///    (like Pinecone) Mojang's own runtime for it.
fn pick_java(
    inst_dir: &Path,
    cfg: &HashMap<String, String>,
    profile: &Profile,
    progress: &Progress,
) -> Result<PathBuf, String> {
    let to_javaw = |p: &str| -> PathBuf {
        let p = PathBuf::from(p.replace('/', "\\"));
        if p.file_name()
            .map_or(false, |n| n.eq_ignore_ascii_case("java.exe"))
        {
            let w = p.with_file_name("javaw.exe");
            if w.exists() {
                return w;
            }
        }
        p
    };
    let override_on = cfg
        .get("OverrideJavaLocation")
        .map_or(false, |v| v == "true");
    if let Some(jp) = cfg.get("JavaPath").filter(|p| !p.is_empty()) {
        if override_on || cfg.get("OverrideJavaLocation").is_none() {
            let p = to_javaw(jp);
            let named_runtime = profile.java_name.as_deref().filter(|name| !name.is_empty());
            let is_named_runtime = named_runtime.map_or(true, |name| {
                p.ancestors()
                    .any(|parent| parent.file_name().and_then(|n| n.to_str()) == Some(name))
            });
            if p.exists() {
                // If automatically cached (not explicit override), verify major version still matches profile
                let compatible = if override_on {
                    true
                } else {
                    is_named_runtime
                        && (profile.java_majors.is_empty()
                            || java_major_of(&p).map_or(true, |m| profile.java_majors.contains(&m)))
                };
                if compatible {
                    return Ok(p);
                }
            }
        }
    }

    // 2. Runtime chosen in the creation screen
    if let Some(vendor) = cfg.get("IkaJavaVendor").filter(|v| !v.is_empty()) {
        let major: u32 = cfg
            .get("IkaJavaMajor")
            .and_then(|m| m.parse().ok())
            .unwrap_or(0);
        let runtime = cfg.get("IkaJavaRuntime").filter(|r| !r.is_empty()).cloned();
        let url = cfg.get("IkaJavaUrl").filter(|u| !u.is_empty()).cloned();
        let folder = java::java_runtime_folder(vendor, major, runtime.as_deref());
        let appdata = PathBuf::from(std::env::var("APPDATA").unwrap_or_default());
        let existing = [
            java::java_install_root(),
            appdata.join("ElyPrismLauncher").join("java"),
            appdata.join("PrismLauncher").join("java"),
        ]
        .into_iter()
        .map(|root| root.join(&folder).join("bin").join("javaw.exe"))
        .find(|p| p.is_file())
        .or_else(|| {
            let runtime = super::instance_catalog::JavaRuntime {
                vendor: vendor.clone(),
                major,
                name: runtime.clone().unwrap_or_default(),
                url: String::new(),
            };
            java::list_java_installations().into_iter()
                .find(|j| j.major == major && runtime.is_installed_at(&j.path))
                .map(|j| PathBuf::from(j.path))
        });
        let javaw = match existing {
            Some(p) => p,
            None => {
                let label = runtime.clone().unwrap_or_else(|| format!("Java {}", major));
                progress.emit("java", &format!("Descargando {}...", label), 0, 0);
                let inst = java::download_java_runtime_inner(vendor, major, url, runtime)?;
                PathBuf::from(inst.path)
            }
        };
        set_cfg_values(
            inst_dir,
            &[
                ("OverrideJavaLocation", "true".to_string()),
                ("JavaPath", javaw.to_string_lossy().replace('\\', "/")),
            ],
        );
        return Ok(javaw);
    }

    // Cretania publishes an exact Mojang runtime name (java-runtime-epsilon),
    // so prefer that folder over an unrelated system Java 25 installation.
    if let Some(runtime_name) = profile.java_name.as_deref().filter(|name| !name.is_empty()) {
        let appdata = PathBuf::from(std::env::var("APPDATA").unwrap_or_default());
        let candidates = [
            java::java_install_root().join(runtime_name),
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join("Lucerion")
                .join("java")
                .join(runtime_name),
            appdata
                .join("ElyPrismLauncher")
                .join("java")
                .join(runtime_name),
            appdata
                .join("PrismLauncher")
                .join("java")
                .join(runtime_name),
        ];
        if let Some(javaw) = candidates
            .into_iter()
            .map(|root| root.join("bin").join("javaw.exe"))
            .find(|p| p.exists())
        {
            set_cfg_values(
                inst_dir,
                &[("JavaPath", javaw.to_string_lossy().replace('\\', "/"))],
            );
            return Ok(javaw);
        }

        // Do not substitute another Java 25 build: Cretania's upstream launcher
        // names Mojang's exact runtime java-runtime-epsilon.
        let major = profile.java_majors.first().copied().unwrap_or(25);
        progress.emit("java", &format!("Descargando {}...", runtime_name), 0, 0);
        let inst = java::download_java_runtime_inner(
            "mojang",
            major,
            None,
            Some(runtime_name.to_string()),
        )?;
        let javaw = PathBuf::from(inst.path);
        set_cfg_values(
            inst_dir,
            &[("JavaPath", javaw.to_string_lossy().replace('\\', "/"))],
        );
        return Ok(javaw);
    }

    // 3. Automatic (persist resolved JavaPath so subsequent launches skip full-disk Java scan)
    let wanted: Vec<u32> = if profile.java_majors.is_empty() {
        vec![8]
    } else {
        profile.java_majors.clone()
    };
    let installed = java::list_java_installations();
    for major in &wanted {
        if let Some(j) = installed
            .iter()
            .find(|j| j.major == *major && Path::new(&j.path).exists())
        {
            let javaw = PathBuf::from(&j.path);
            set_cfg_values(
                inst_dir,
                &[("JavaPath", javaw.to_string_lossy().replace('\\', "/"))],
            );
            return Ok(javaw);
        }
    }

    let major = wanted[0];
    progress.emit(
        "java",
        &format!("Descargando Java {} de Mojang...", major),
        0,
        0,
    );
    let inst = java::download_java_runtime_inner("mojang", major, None, profile.java_name.clone())?;
    let javaw = PathBuf::from(inst.path);
    set_cfg_values(
        inst_dir,
        &[("JavaPath", javaw.to_string_lossy().replace('\\', "/"))],
    );
    Ok(javaw)
}

fn copy_if_missing(src: &Path, dst: &Path) {
    if dst.exists() {
        return;
    }
    if let Some(parent) = dst.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::copy(src, dst);
}

/// Extracts old-style native jars (LWJGL 2 era) into `natives_dir`. LWJGL 3 natives are
/// regular classpath jars that LWJGL unpacks by itself.
/// Uses `.ika-natives.stamp` to avoid deleting and re-extracting DLLs on every launch
/// (which otherwise triggers Windows Defender real-time scanning on every startup).
fn extract_natives(jars: &[(PathBuf, Vec<String>)], natives_dir: &Path) -> Result<(), String> {
    let stamp_file = natives_dir.join(".ika-natives.stamp");
    let mut stamp = String::new();
    for (jar, excludes) in jars {
        let len = std::fs::metadata(jar).map(|m| m.len()).unwrap_or(0);
        stamp.push_str(&format!(
            "{}|{}|{}\n",
            jar.display(),
            len,
            excludes.join(",")
        ));
    }
    if natives_dir.is_dir() {
        if let Ok(existing_stamp) = std::fs::read_to_string(&stamp_file) {
            if existing_stamp == stamp {
                return Ok(());
            }
        }
    }
    let _ = std::fs::remove_dir_all(natives_dir);
    std::fs::create_dir_all(natives_dir).map_err(|e| e.to_string())?;
    for (jar, excludes) in jars {
        let file = std::fs::File::open(jar).map_err(|e| format!("{}: {}", jar.display(), e))?;
        let mut zip =
            zip::ZipArchive::new(file).map_err(|e| format!("{}: {}", jar.display(), e))?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().to_string();
            if entry.is_dir()
                || name.starts_with("META-INF/")
                || excludes.iter().any(|x| name.starts_with(x.as_str()))
            {
                continue;
            }
            let Some(rel) = entry.enclosed_name() else {
                continue;
            };
            let out = natives_dir.join(rel);
            if let Some(parent) = out.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let mut f = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
        }
    }
    let _ = std::fs::write(&stamp_file, stamp);
    Ok(())
}

/// Builds `<instance>/bin/minecraft.jar` from the vanilla jar plus the instance's jar mods,
/// like Prism's MMCZip::createModdedJar: mods in reverse order (so later ones win), then the
/// vanilla jar without META-INF (its signatures would reject the modified classes).
fn build_modded_jar(
    inst_dir: &Path,
    vanilla: &Path,
    jar_mods: &[String],
) -> Result<PathBuf, String> {
    let mods: Vec<PathBuf> = jar_mods
        .iter()
        .map(|f| inst_dir.join("jarmods").join(f))
        .collect();
    if let Some(missing) = mods.iter().find(|m| !m.exists()) {
        return Err(format!("Falta el jar mod {}", missing.display()));
    }
    let out = inst_dir.join("bin").join("minecraft.jar");
    // Reuse the previous build while none of its inputs changed.
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    if let Some(built) = modified(&out) {
        if mods
            .iter()
            .chain(std::iter::once(&vanilla.to_path_buf()))
            .all(|p| modified(p).map_or(false, |t| t <= built))
        {
            return Ok(out);
        }
    }
    std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
    let tmp = out.with_extension("jar.part");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&tmp).map_err(|e| e.to_string())?);
    let mut added: HashSet<String> = HashSet::new();
    let mut sources: Vec<(&Path, bool)> = mods.iter().rev().map(|m| (m.as_path(), false)).collect();
    sources.push((vanilla, true));
    for (src, is_vanilla) in sources {
        let file = std::fs::File::open(src).map_err(|e| format!("{}: {}", src.display(), e))?;
        let mut zip =
            zip::ZipArchive::new(file).map_err(|e| format!("{}: {}", src.display(), e))?;
        for i in 0..zip.len() {
            let entry = zip.by_index_raw(i).map_err(|e| e.to_string())?;
            let name = entry.name().to_string();
            if (is_vanilla && name.contains("META-INF")) || !added.insert(name) {
                continue;
            }
            writer.raw_copy_file(entry).map_err(|e| e.to_string())?;
        }
    }
    writer.finish().map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &out).map_err(|e| e.to_string())?;
    Ok(out)
}

// ---------------------------------------------------------------------------------------
// Install + launch
// ---------------------------------------------------------------------------------------

/// Crash report written by this launch (newest file in crash-reports/ after `since`).
fn find_crash_report(game_dir: &Path, since: std::time::SystemTime) -> Option<PathBuf> {
    std::fs::read_dir(game_dir.join("crash-reports"))
        .ok()?
        .flatten()
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            (modified >= since).then(|| (modified, e.path()))
        })
        .max_by_key(|(m, _)| *m)
        .map(|(_, p)| p)
}

/// "Description: ..." plus the exception and its first frames from a crash report.
pub(crate) fn crash_report_summary(report: &Path) -> Option<String> {
    let text = std::fs::read_to_string(report).ok()?;
    let mut out: Vec<&str> = Vec::new();
    let mut in_head = false;
    for line in text.lines() {
        if line.starts_with("Description:") {
            in_head = true;
            out.push(line.trim());
            continue;
        }
        if in_head {
            if line.starts_with("A detailed walkthrough") {
                break;
            }
            if !line.trim().is_empty() {
                out.push(line.trim_end());
            }
            if out.len() >= 12 {
                break;
            }
        }
    }
    (!out.is_empty()).then(|| out.join("\n"))
}

/// Without a crash report: the last error-looking lines of the game's output.
fn output_error_tail(log: &Path) -> String {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('<'))
        .collect();
    let interesting: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| {
            let lower = l.to_lowercase();
            lower.contains("error")
                || lower.contains("exception")
                || lower.contains("caused by")
                || lower.starts_with("\tat ")
        })
        .collect();
    let pick = if interesting.is_empty() {
        &lines
    } else {
        &interesting
    };
    pick[pick.len().saturating_sub(10)..].join("\n")
}

/// Downloads everything the instance needs and starts the game process.
pub(super) fn run(
    notify: &(dyn Fn(&str, usize, usize) + Sync),
    inst_dir: &Path,
    username: &str,
    skin: Option<super::Skin>,
    spawned: impl FnOnce(u32),
    started: impl FnOnce(),
) -> Result<(), String> {
    run_in(&ika_root(), notify, inst_dir, username, skin, spawned, started)
}

fn skin_agent(root: &Path, agent: &ureq::Agent, name: &str, uuid: &str, skin: super::Skin) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let jar = root.join("authlib-injector.jar");
    let sum = root.join("authlib-injector.sha256");
    let verify = |bytes: &[u8], digest: &str| format!("{:x}", Sha256::digest(bytes)).eq_ignore_ascii_case(digest.trim());
    let cached = jar.metadata().is_ok_and(|m| m.len() <= 8 * 1024 * 1024)
        && std::fs::read(&jar).ok().zip(std::fs::read_to_string(&sum).ok())
           .is_some_and(|(bytes, hash)| verify(&bytes, &hash));
    if !cached {
        let meta: Value = serde_json::from_slice(&http_get_bytes(agent, "https://authlib-injector.yushi.moe/artifact/latest.json")?)
            .map_err(|e| e.to_string())?;
        let url = meta["download_url"].as_str().filter(|s|s.starts_with("https://")).ok_or("URL de authlib-injector inválida")?;
        let hash = meta.pointer("/checksums/sha256").and_then(Value::as_str).ok_or("Falta el checksum de authlib-injector")?;
        let bytes = http_get_bytes(agent, url)?;
        if !verify(&bytes, hash) { return Err("Checksum de authlib-injector incorrecto".into()) }
        std::fs::create_dir_all(root).map_err(|e|e.to_string())?;
        for (path, data) in [(&jar, bytes.as_slice()), (&sum, hash.as_bytes())] {
            let mut f = tempfile::NamedTempFile::new_in(root).map_err(|e|e.to_string())?;
            std::io::Write::write_all(&mut f,data).map_err(|e|e.to_string())?;
            f.persist(path).map_err(|e|e.error.to_string())?;
        }
    }
    let url = super::skin_server::register(name, uuid, skin.png, skin.slim)?;
    Ok(format!("-javaagent:{}={url}",jar.display()))
}

/// IkaBoost is on by default for Forge 1.20.1 (the setup measured with Linggango:
/// ~100 s -> ~62 s with Temurin 25). `IkaBoost=true` turns it on for other
/// ModLauncher versions, `IkaBoost=false` turns it off.
fn boost_enabled(cfg: &HashMap<String, String>, profile: &Profile) -> bool {
    if std::env::var_os("IKA_BOOST_OFF").is_some() {
        return false;
    }
    match cfg.get("IkaBoost").map(String::as_str) {
        Some("true") => true,
        Some("false") => false,
        _ => profile.mc_version == "1.20.1" && profile.main_class.contains("forgewrapper"),
    }
}

/// Same rule from the instance files alone (no component metadata needed).
pub(super) fn boost_enabled_dir(inst_dir: &Path) -> bool {
    let cfg = read_cfg(inst_dir);
    match cfg.get("IkaBoost").map(String::as_str) {
        Some("true") => return true,
        Some("false") => return false,
        _ => {}
    }
    let Ok(pack) = std::fs::read_to_string(inst_dir.join("mmc-pack.json")) else { return false };
    let Ok(pack) = serde_json::from_str::<Value>(&pack) else { return false };
    let comps = pack.get("components").and_then(|c| c.as_array()).cloned().unwrap_or_default();
    let has = |uid: &str, version: Option<&str>| comps.iter().any(|c| {
        c.get("uid").and_then(|u| u.as_str()) == Some(uid)
            && version.is_none_or(|v| c.get("version").or_else(|| c.get("cachedVersion")).and_then(|x| x.as_str()) == Some(v))
    });
    has("net.minecraft", Some("1.20.1")) && has("net.minecraftforge", None)
}

/// Embedded Java agent (tools/ika-boost) that caches ModLauncher's transformed classes.
const BOOST_AGENT: &[u8] = include_bytes!("ika-boost.jar");

/// `-javaagent` argument for the class cache of a ModLauncher (Forge/NeoForge) instance.
/// The cache directory name is a fingerprint of everything Mixin and the other
/// transformers can depend on besides the class bytes themselves (those are checked
/// per class by the agent): mod jars, configs, the launch libraries and the JVM.
/// See `boost_enabled` for when it is used.
fn boost_agent(
    root: &Path,
    inst_dir: &Path,
    gdir: &Path,
    cfg: &HashMap<String, String>,
    profile: &Profile,
    java: &Path,
    classpath: &[String],
) -> Option<String> {
    use rayon::prelude::*;
    use sha2::{Digest, Sha256};
    if !boost_enabled(cfg, profile) {
        return None;
    }
    if !profile.main_class.contains("forgewrapper") && !profile.main_class.contains("bootstraplauncher") {
        return None;
    }
    let jar = root.join("ika-boost").join(format!("ika-boost-{:x}.jar", Sha256::digest(BOOST_AGENT)).get(..24)?.to_string() + ".jar");
    if !jar.is_file() {
        std::fs::create_dir_all(jar.parent()?).ok()?;
        let mut f = tempfile::NamedTempFile::new_in(jar.parent()?).ok()?;
        std::io::Write::write_all(&mut f, BOOST_AGENT).ok()?;
        f.persist(&jar).ok()?;
    }
    let mut files: Vec<PathBuf> = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() { walk(&p, out) } else { out.push(p) }
            }
        }
    }
    walk(&gdir.join("config"), &mut files);
    walk(&gdir.join("defaultconfigs"), &mut files);
    files.sort();
    // Configs are rewritten (same content, new mtime) on every launch: hash contents.
    let config_hashes: Vec<String> = files
        .par_iter()
        .map(|p| {
            let mut bytes = std::fs::read(p).unwrap_or_default();
            // Several mods rewrite their config on every launch: Java .properties put the
            // date in a comment and hash-map based writers reorder keys. Hash the sorted
            // non-comment lines so only real value changes invalidate the cache.
            if let Ok(text) = std::str::from_utf8(&bytes) {
                let props = p.extension().is_some_and(|e| e == "properties");
                let mut lines: Vec<&str> = text
                    .lines()
                    .map(|l| l.trim_end().trim_end_matches(','))
                    .filter(|l| !(props && (l.starts_with('#') || l.starts_with('!'))))
                    .collect();
                lines.sort_unstable();
                bytes = lines.join("\n").into_bytes();
            }
            format!("{}={:x}", p.strip_prefix(gdir).unwrap_or(p).display(), Sha256::digest(&bytes))
        })
        .collect();
    let mut h = Sha256::new();
    let mut manifest = String::new();
    // Bump when the agent changes what ends up in the class cache (not for
    // patches that only touch loading speed).
    h.update(b"ika-boost-class-format-5\0");
    let meta_line = |p: &Path| {
        let m = std::fs::metadata(p).ok();
        let mtime = m.as_ref().and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
        format!("{}|{}|{}\n", p.display(), m.map_or(0, |m| m.len()), mtime)
    };
    let mut mods: Vec<PathBuf> = Vec::new();
    walk(&gdir.join("mods"), &mut mods);
    mods.sort();
    for p in mods.iter().map(PathBuf::as_path).chain(classpath.iter().map(Path::new)).chain([java]) {
        let line = meta_line(p);
        h.update(line.as_bytes());
        manifest.push_str(&line);
    }
    for c in &config_hashes {
        h.update(c.as_bytes());
        h.update(b"\n");
        manifest.push_str(c);
        manifest.push('\n');
    }
    for a in &profile.jvm_args {
        h.update(a.as_bytes());
    }
    let key = format!("{:x}", h.finalize());
    let name: String = inst_dir.file_name()?.to_string_lossy().chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let base = root.join("cache").join("boost");
    let dir = base.join(format!("{name}-{}", &key[..16]));
    // Keep one cache per instance.
    if let Ok(rd) = std::fs::read_dir(&base).map_err(|_| ()).and_then(|rd| if cfg!(test) { Err(()) } else { Ok(rd) }) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with(&format!("{name}-")) && e.path() != dir {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("fingerprint.txt"), &manifest);
    if !cfg!(test) || std::env::var_os("IKA_BENCH").is_some() {
        let _ = std::fs::write(base.join(format!("{name}.dir")), dir.to_string_lossy().as_bytes());
        let _ = std::fs::write(dir.join("mods.txt"), mods_listing(gdir));
    }
    let _ = std::fs::write(base.join(format!("{name}.{}fingerprint.txt", if cfg!(test) { "test-" } else { "last-" })), &manifest);
    Some(format!("-javaagent:{}={}", jar.display(), dir.display()))
}
/// True when the instance uses IkaBoost and its class cache is missing,
/// unconfirmed or stale (the agent writes `ready` after a launch that served
/// most classes from a complete cache).
pub(super) fn boost_needs_prewarm(inst_dir: &Path) -> bool {
    if !boost_enabled_dir(inst_dir) {
        return false;
    }
    // The launch records which cache directory (fingerprint) it used. A mod or
    // config change produces a new directory without `ready`.
    let name: String = inst_dir.file_name().map(|n| n.to_string_lossy().chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()).unwrap_or_default();
    let base = ika_root().join("cache").join("boost");
    let Ok(dir) = std::fs::read_to_string(base.join(format!("{name}.dir"))) else { return true };
    let dir = PathBuf::from(dir.trim());
    if !dir.join("ready").is_file() {
        return true;
    }
    // Still the same mods? (cheap check: mods folder listing with sizes/times)
    std::fs::read_to_string(dir.join("mods.txt")).ok().as_deref() != Some(mods_listing(&game_dir(inst_dir)).as_str())
}

fn mods_listing(gdir: &Path) -> String {
    let mut out: Vec<String> = std::fs::read_dir(gdir.join("mods")).into_iter().flatten().flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis();
            Some(format!("{}|{}|{}", e.file_name().to_string_lossy(), m.len(), t))
        })
        .collect();
    out.sort();
    out.join("\n")
}

/// Set by a real launch to stop a background warm-up of any instance.
static PREWARM_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(super) fn cancel_prewarm() {
    PREWARM_CANCEL.store(true, Ordering::SeqCst);
}

thread_local! {
    /// Set while `prewarm` runs: the game is started on an invisible desktop
    /// only to fill the IkaBoost caches, and it quits by itself.
    static PREWARM: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Starts the instance once, invisibly, so the IkaBoost class cache gets built
/// and confirmed before the player presses Play. Uses an off-screen Win32
/// desktop: the game window exists (Forge needs an OpenGL context) but is never
/// shown. Returns when the game exits; it stops by itself once class loading
/// is done.
pub(super) fn prewarm(inst_dir: &Path, username: &str) -> Result<(), String> {
    if !boost_enabled_dir(inst_dir) {
        return Ok(());
    }
    PREWARM.with(|p| p.set(true));
    let result = run_in(&ika_root(), &|_, _, _| {}, inst_dir, username, None, |_| {}, || {});
    PREWARM.with(|p| p.set(false));
    result
}

#[cfg(windows)]
fn hidden_desktop() -> Option<String> {
    use windows::Win32::System::StationsAndDesktops::{CreateDesktopW, DESKTOP_CONTROL_FLAGS};
    use windows::core::PCWSTR;
    let name = "IkaPrewarm";
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    // GENERIC_ALL. The desktop object stays alive for as long as a process uses it.
    let desk = unsafe {
        CreateDesktopW(PCWSTR(wide.as_ptr()), PCWSTR::null(), None, DESKTOP_CONTROL_FLAGS(0), 0x10000000, None)
    };
    desk.ok().map(|_| format!("WinSta0\\{name}"))
}
/// std::process::Command has no STARTUPINFO.lpDesktop; spawn through
/// CreateProcessW when a desktop is given.
fn spawn_on_desktop(cmd: std::process::Command, desktop: Option<&str>) -> std::io::Result<SpawnedChild> {
    #[cfg(windows)]
    if let Some(desktop) = desktop {
        use windows::Win32::System::Threading::{CreateProcessW, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION, STARTUPINFOW};
        use windows::core::{PCWSTR, PWSTR};
        let quote = |s: &str| -> String {
            if !s.is_empty() && !s.contains([' ', '\t', '"']) { return s.to_string(); }
            let mut out = String::from("\"");
            let mut slashes = 0;
            for ch in s.chars() {
                match ch {
                    '\\' => slashes += 1,
                    '"' => { out.push_str(&"\\".repeat(slashes * 2 + 1)); out.push('"'); slashes = 0; continue; }
                    _ => { out.push_str(&"\\".repeat(slashes)); slashes = 0; out.push(ch); continue; }
                }
            }
            out.push_str(&"\\".repeat(slashes * 2));
            out.push('"');
            out
        };
        let program = cmd.get_program().to_string_lossy().to_string();
        let mut line = quote(&program);
        for a in cmd.get_args() {
            line.push(' ');
            line.push_str(&quote(&a.to_string_lossy()));
        }
        let mut line_w: Vec<u16> = line.encode_utf16().chain(Some(0)).collect();
        let mut desk_w: Vec<u16> = desktop.encode_utf16().chain(Some(0)).collect();
        let dir_w: Option<Vec<u16>> = cmd.get_current_dir().map(|d| d.as_os_str().to_string_lossy().encode_utf16().chain(Some(0)).collect());
        // Environment: inherit, plus the overrides from cmd.
        let mut env: std::collections::BTreeMap<String, String> = std::env::vars().collect();
        for (k, v) in cmd.get_envs() {
            match v {
                Some(v) => { env.insert(k.to_string_lossy().to_string(), v.to_string_lossy().to_string()); }
                None => { env.remove(&*k.to_string_lossy()); }
            }
        }
        let mut env_w: Vec<u16> = Vec::new();
        for (k, v) in &env {
            env_w.extend(format!("{k}={v}").encode_utf16());
            env_w.push(0);
        }
        env_w.push(0);
        let mut si = STARTUPINFOW::default();
        si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        si.lpDesktop = PWSTR(desk_w.as_mut_ptr());
        let mut pi = PROCESS_INFORMATION::default();
        // CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS | CREATE_UNICODE_ENVIRONMENT
        let flags = PROCESS_CREATION_FLAGS(0x08000000 | 0x00004000 | 0x00000400);
        unsafe {
            CreateProcessW(
                PCWSTR::null(),
                Some(PWSTR(line_w.as_mut_ptr())),
                None,
                None,
                false,
                flags,
                Some(env_w.as_ptr() as *const _),
                dir_w.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
                &si,
                &mut pi,
            )
        }
        .map_err(|e| std::io::Error::other(e.to_string()))?;
        unsafe { let _ = windows::Win32::Foundation::CloseHandle(pi.hThread); }
        return Ok(SpawnedChild::Raw(pi.hProcess));
    }
    let mut cmd = cmd;
    cmd.spawn().map(SpawnedChild::Std)
}

enum SpawnedChild {
    Std(std::process::Child),
    #[cfg(windows)]
    Raw(windows::Win32::Foundation::HANDLE),
}

impl SpawnedChild {
    fn try_wait(&mut self) -> std::io::Result<Option<()>> {
        match self {
            SpawnedChild::Std(c) => c.try_wait().map(|s| s.map(|_| ())),
            #[cfg(windows)]
            SpawnedChild::Raw(h) => {
                use windows::Win32::System::Threading::WaitForSingleObject;
                let r = unsafe { WaitForSingleObject(*h, 0) };
                Ok((r.0 == 0).then_some(()))
            }
        }
    }
    fn kill(&mut self) -> std::io::Result<()> {
        match self {
            SpawnedChild::Std(c) => c.kill(),
            #[cfg(windows)]
            SpawnedChild::Raw(h) => unsafe {
                windows::Win32::System::Threading::TerminateProcess(*h, 1).map_err(|e| std::io::Error::other(e.to_string()))
            },
        }
    }
}
/// IkaBoost: Forge 1.20.x (ModLauncher 10) starts much faster on a recent Java.
/// Measured with Linggango (455 mods): ~88 s on Mojang's Java 17, ~83 s on
/// Temurin 21, ~63 s on Temurin 25. Mods compiled for 17 run unchanged.
/// Uses an installed Temurin 25 (downloads it the first time; Temurin 21 if that fails)
/// unless the instance's Java was chosen by hand in Ika (OverrideJavaLocation=true)
/// or IkaBoostJava=false.
fn boost_java(cfg: &HashMap<String, String>, profile: &Profile, current: &Path) -> Option<PathBuf> {
    if !boost_enabled(cfg, profile)
        || cfg.get("IkaBoostJava").is_some_and(|v| v == "false")
        || cfg.get("OverrideJavaLocation").is_some_and(|v| v == "true")
    {
        return None;
    }
    // Only ModLauncher-based Forge on Java 17 (1.18-1.20.x).
    if !profile.main_class.contains("forgewrapper") || !profile.java_majors.contains(&17) {
        return None;
    }
    if java_major_of(current).is_some_and(|m| m >= 25) {
        return None;
    }
    let installed = java::list_java_installations();
    let pick = |major: u32| {
        installed.iter()
            .filter(|j| j.major == major && Path::new(&j.path).is_file())
            .find(|j| {
                let home = Path::new(&j.path).parent().and_then(|b| b.parent());
                home.and_then(|h| std::fs::read_to_string(h.join("release")).ok())
                    .is_some_and(|r| r.contains("Eclipse Adoptium"))
            })
            .map(|j| PathBuf::from(&j.path))
    };
    pick(25)
        .or_else(|| java::download_java_runtime_inner("adoptium", 25, None, None).ok().map(|j| PathBuf::from(j.path)))
        .or_else(|| pick(21))
}

fn run_in(
    root: &Path,
    notify: &(dyn Fn(&str, usize, usize) + Sync),
    inst_dir: &Path,
    username: &str,
    skin: Option<super::Skin>,
    spawned: impl FnOnce(u32),
    started: impl FnOnce(),
) -> Result<(), String> {
    let progress = &Progress(notify);
    if !inst_dir.join("mmc-pack.json").exists() {
        return Err("No se encontró la instancia (falta mmc-pack.json).".to_string());
    }
    if !super::valid_username(username) {
        return Err("Nombre de Minecraft inválido.".into());
    }
    let inst_dir = inst_dir.to_path_buf();
    let username = username.to_string();
    let inst_name = inst_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let agent = agent();

    let libs_dir = root.join("libraries");
    let assets_dir = root.join("assets");
    let cfg = read_cfg(&inst_dir);
    let gdir = game_dir(&inst_dir);
    std::fs::create_dir_all(&gdir).map_err(|e| e.to_string())?;
    // First run of the instance: go straight to the title screen instead of the
    // accessibility onboarding screen (1.19.3+, the one where the narrator starts
    // talking). The game fills in every other option with its defaults; older versions
    // just ignore the unknown key. An existing options.txt is never touched.
    let options = gdir.join("options.txt");
    if !options.exists() {
        let _ = std::fs::write(&options, "onboardAccessibility:false\nnarrator:0\n");
    }

    // Ensure any mods located in loader/version subfolders (e.g. Lunar Client mods/fabric-26.2)
    // are present in gdir/mods so Minecraft and the modloader load them on launch.
    let root_mods = inst_dir.join("mods");
    let target_mods = gdir.join("mods");
    let _ = std::fs::create_dir_all(&target_mods);
    if root_mods.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&root_mods) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() && p != target_mods {
                    if let Ok(sub) = std::fs::read_dir(&p) {
                        for se in sub.flatten() {
                            let sep = se.path();
                            let name = se.file_name();
                            let n = name.to_string_lossy().to_lowercase();
                            if (n.ends_with(".jar") || n.ends_with(".jar.disabled"))
                                && !n.starts_with('.')
                            {
                                let dest = target_mods.join(&name);
                                if !dest.exists() {
                                    let _ = std::fs::copy(&sep, &dest);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 1. Components -> launch profile
    progress.emit("meta", "Leyendo versiones...", 0, 0);
    let components = resolve_components(&agent, &inst_dir)?;
    let mut profile = Profile::default();
    for (comp, meta) in &components {
        profile.apply(comp, meta);
    }

    // Pre-1.6 clients (Prism's "legacyLaunch" trait): Prism starts them with its own wrapper,
    // so its meta has no main class for them. Start them like the official launcher does,
    // with the main class and launchwrapper libraries from Mojang's version file.
    if profile.main_class.is_empty() && profile.traits.contains("legacyLaunch") {
        let mojang = load_mojang_version(&agent, &profile.mc_version)?;
        profile.main_class = mojang
            .get("mainClass")
            .and_then(|v| v.as_str())
            .unwrap_or("net.minecraft.launchwrapper.Launch")
            .to_string();
        for lib in mojang
            .get("libraries")
            .and_then(|l| l.as_array())
            .into_iter()
            .flatten()
        {
            let name = lib.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let key = library_key(name);
            if lib.get("natives").is_none()
                && !profile.libraries.iter().any(|l| {
                    library_key(l.get("name").and_then(|n| n.as_str()).unwrap_or("")) == key
                })
            {
                profile.libraries.push(lib.clone());
            }
        }
    }
    if profile.main_class.is_empty() {
        return Err(
            "Esta versión de Minecraft no se puede iniciar (sin clase principal).".to_string(),
        );
    }

    // 2. Library + main jar downloads
    let mut downloads: Vec<Download> = Vec::new();
    let mut classpath: Vec<PathBuf> = Vec::new();
    let mut native_jars: Vec<(PathBuf, Vec<String>)> = Vec::new();
    for lib in profile.libraries.iter().filter(|l| rules_allow(l)) {
        let has_artifact =
            lib.pointer("/downloads/artifact").is_some() || lib.get("natives").is_none();
        if has_artifact {
            if let Some(d) = library_download(lib, &libs_dir, None) {
                classpath.push(d.path.clone());
                // Empty URL: the file is generated at first launch (Forge processors via ForgeWrapper).
                if !d.url.is_empty() && !d.url.ends_with('/') {
                    downloads.push(d);
                }
            }
        }
        if let Some(cls) = native_classifier(lib) {
            if let Some(d) = library_download(lib, &libs_dir, Some(&cls)) {
                let excludes = str_list(lib.pointer("/extract/exclude"));
                native_jars.push((d.path.clone(), excludes));
                downloads.push(d);
            }
        }
    }
    for f in profile.maven_files.iter().filter(|l| rules_allow(l)) {
        if let Some(d) = library_download(f, &libs_dir, None) {
            if !d.url.is_empty() {
                downloads.push(d);
            }
        }
    }
    let main_jar = profile
        .main_jar
        .as_ref()
        .and_then(|j| library_download(j, &libs_dir, None))
        .ok_or("No se encontró el jar de Minecraft en los metadatos")?;
    downloads.push(main_jar.clone());
    let main_jar_slot = classpath.len();
    classpath.push(main_jar.path.clone());

    // Cretania's Java agent also exposes cretania.lanzadera.* to its custom
    // classloader. Java agents are not automatically visible on the application
    // classpath, so include lanzadera.jar explicitly as well as -javaagent.
    if let Some(agent_path) = profile
        .jvm_args
        .iter()
        .find_map(|arg| arg.strip_prefix("-javaagent:"))
    {
        let agent_path = PathBuf::from(agent_path);
        if agent_path.exists() && !classpath.iter().any(|path| path == &agent_path) {
            classpath.push(agent_path);
        }
    }

    // Log4j config (also the Log4Shell fix for old versions)
    let mut logging_arg: Option<String> = None;
    if let Some(log) = &profile.logging {
        if let (Some(id), Some(url)) = (
            log.pointer("/file/id").and_then(|v| v.as_str()),
            log.pointer("/file/url").and_then(|v| v.as_str()),
        ) {
            let path = assets_dir.join("log_configs").join(id);
            downloads.push(Download {
                url: url.to_string(),
                path: path.clone(),
                sha1: log
                    .pointer("/file/sha1")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                size: log.pointer("/file/size").and_then(|v| v.as_u64()),
            });
            if let Some(arg) = log.get("argument").and_then(|v| v.as_str()) {
                // log4j expects a URI here; a plain Windows path works but logs
                // "Error parsing URI" on every start.
                let uri = format!("file:///{}", path.to_string_lossy().replace('\\', "/"));
                logging_arg = Some(arg.replace("${path}", &uri));
            }
        }
    }
    download_all(
        downloads,
        progress,
        "libraries",
        "Descargando Minecraft y librerías",
    )?;

    if !profile.jar_mods.is_empty() {
        progress.emit("jarmods", "Aplicando jar mods...", 0, 0);
        classpath[main_jar_slot] = build_modded_jar(&inst_dir, &main_jar.path, &profile.jar_mods)?;
    }

    // 3. Assets (skip parsing 450 KB JSON + 4,000 stat calls if already verified via .ika-ok stamp)
    let asset_index = profile
        .asset_index
        .clone()
        .ok_or("Faltan los assets en los metadatos")?;
    let asset_id = asset_index
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("legacy")
        .to_string();
    let index_path = assets_dir
        .join("indexes")
        .join(format!("{}.json", asset_id));
    let index_stamp_path = assets_dir
        .join("indexes")
        .join(format!("{}.ika-ok", asset_id));
    let expected_index_sha1 = asset_index
        .get("sha1")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    let index_dl = Download {
        url: asset_index
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        path: index_path.clone(),
        sha1: asset_index
            .get("sha1")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        size: asset_index.get("size").and_then(|v| v.as_u64()),
    };
    let is_legacy_layout = asset_id == "legacy" || asset_id == "pre-1.6";
    let fast_verified = !is_legacy_layout
        && index_dl.satisfied()
        && std::fs::read_to_string(&index_stamp_path)
            .map(|s| s.trim() == expected_index_sha1 && !expected_index_sha1.is_empty())
            .unwrap_or(false);

    let game_assets = if fast_verified {
        assets_dir.clone()
    } else {
        let index_ok = index_dl.satisfied()
            && index_dl.sha1.as_ref().map_or(true, |s| {
                sha1_file(&index_path).map_or(false, |h| h.eq_ignore_ascii_case(s))
            });
        if !index_ok {
            progress.emit("assets", "Descargando índice de assets...", 0, 0);
            index_dl.fetch(&agent)?;
        }
        let index: Value =
            serde_json::from_str(&std::fs::read_to_string(&index_path).map_err(|e| e.to_string())?)
                .map_err(|e| format!("Índice de assets inválido: {}", e))?;
        let objects = index
            .get("objects")
            .and_then(|o| o.as_object())
            .cloned()
            .unwrap_or_default();
        let mut asset_dls = Vec::with_capacity(objects.len());
        for obj in objects.values() {
            let Some(hash) = obj.get("hash").and_then(|h| h.as_str()) else {
                continue;
            };
            if hash.len() < 2 {
                continue;
            }
            asset_dls.push(Download {
                url: format!("{}/{}/{}", RESOURCES_URL, &hash[..2], hash),
                path: assets_dir.join("objects").join(&hash[..2]).join(hash),
                sha1: Some(hash.to_string()),
                size: obj.get("size").and_then(|s| s.as_u64()),
            });
        }
        download_all(asset_dls, progress, "assets", "Descargando assets")?;

        // Old asset layouts: "virtual" (1.6) and "map_to_resources" (pre-1.6) need real files.
        let is_virtual = index
            .get("virtual")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let to_resources = index
            .get("map_to_resources")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let ga = if to_resources {
            gdir.join("resources")
        } else if is_virtual {
            assets_dir.join("virtual").join(&asset_id)
        } else {
            assets_dir.clone()
        };
        if is_virtual || to_resources {
            progress.emit("assets", "Preparando assets antiguos...", 0, 0);
            for (name, obj) in &objects {
                if let Some(hash) = obj.get("hash").and_then(|h| h.as_str()) {
                    let src = assets_dir.join("objects").join(&hash[..2]).join(hash);
                    copy_if_missing(&src, &ga.join(name.replace('/', "\\")));
                }
            }
        } else if !expected_index_sha1.is_empty() {
            let _ = std::fs::write(&index_stamp_path, &expected_index_sha1);
        }
        ga
    };

    // 4. Java
    let mut java = pick_java(&inst_dir, &cfg, &profile, progress)?;
    if let Some(fast) = boost_java(&cfg, &profile, &java) {
        progress.emit("java", "IkaBoost: usando un Java más rápido para Forge", 0, 0);
        java = fast;
    }
    // Say which Java is used (vendor + exact version from its release file) in the console.
    {
        let release = java
            .parent()
            .and_then(|b| b.parent())
            .and_then(|home| std::fs::read_to_string(home.join("release")).ok())
            .unwrap_or_default();
        let field = |k: &str| {
            release
                .lines()
                .find_map(|l| l.strip_prefix(&format!("{}=", k)))
                .map(|v| v.trim().trim_matches('"').to_string())
                .unwrap_or_default()
        };
        let vendor = field("IMPLEMENTOR");
        let version = Some(field("SEMANTIC_VERSION"))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| field("JAVA_VERSION"));
        let vendor = if vendor.is_empty() {
            "Java".to_string()
        } else {
            vendor
        };
        progress.emit(
            "java-info",
            &format!("Java: {} {} — {}", vendor, version, java.display()),
            0,
            0,
        );
    }
    if let (Some(major), Some(&needed)) = (java_major_of(&java), profile.java_majors.iter().min()) {
        if major < needed {
            return Err(format!(
                "Minecraft {} necesita Java {} o superior, pero la instancia usa Java {}.",
                profile.mc_version, needed, major
            ));
        }
    }

    // 5. Natives
    let natives_dir = inst_dir.join("natives");
    extract_natives(&native_jars, &natives_dir)?;

    // 6. Command line (same arguments Prism builds)
    let uuid = offline_uuid(&username);
    let skin_agent = skin.map(|skin| {
        progress.emit("skin", "Preparando tu skin...", 0, 0);
        skin_agent(root, &agent, &username, &uuid, skin)
            .map_err(|e| format!("No se pudo aplicar tu skin: {e}"))
    }).transpose()?;

    let sep = ";";
    let cp: Vec<String> = {
        let mut seen = HashSet::new();
        classpath
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .filter(|p| seen.insert(p.to_lowercase()))
            .collect()
    };
    let mut tokens: HashMap<&str, String> = HashMap::new();
    tokens.insert("auth_player_name", username.clone());
    tokens.insert("auth_uuid", uuid.clone());
    tokens.insert("auth_access_token", "0".to_string());
    tokens.insert("auth_session", "0".to_string());
    tokens.insert("auth_xuid", "0".to_string());
    tokens.insert("clientid", "0".to_string());
    tokens.insert("user_type", if skin_agent.is_some() {"mojang"} else {"legacy"}.to_string());
    tokens.insert("user_properties", "{}".to_string());
    tokens.insert("version_name", profile.mc_version.clone());
    tokens.insert("version_type", profile.version_type.clone());
    tokens.insert("profile_name", inst_name.clone());
    tokens.insert("game_directory", gdir.to_string_lossy().to_string());
    tokens.insert("assets_root", assets_dir.to_string_lossy().to_string());
    tokens.insert("assets_index_name", asset_id.clone());
    tokens.insert("game_assets", game_assets.to_string_lossy().to_string());
    tokens.insert("library_directory", libs_dir.to_string_lossy().to_string());
    tokens.insert(
        "natives_directory",
        natives_dir.to_string_lossy().to_string(),
    );
    tokens.insert("classpath_separator", sep.to_string());
    tokens.insert("launcher_name", "IkaLauncher".to_string());
    tokens.insert("launcher_version", env!("CARGO_PKG_VERSION").to_string());

    let mut args: Vec<String> = vec!["-Duser.language=en".to_string()];
    let mut user_jvm_args = Vec::new();
    if cfg.get("OverrideJavaArgs").map_or(true, |v| v == "true") {
        if let Some(extra) = cfg.get("JvmArgs") {
            user_jvm_args = split_args(&unescape_ini(extra));
        }
    }
    let has_custom_gc = user_jvm_args
        .iter()
        .any(|a| a.contains("GC") || a.starts_with("-XX:+Use"));
    if !has_custom_gc {
        // Fast-boot & G1GC flags tuned for rapid classloading and Minecraft window creation
        args.extend([
            "-XX:+UnlockExperimentalVMOptions".to_string(),
            "-XX:+TieredCompilation".to_string(),
            "-XX:+AlwaysActAsServerClassMachine".to_string(),
            "-XX:+UseG1GC".to_string(),
            "-XX:+ParallelRefProcEnabled".to_string(),
            "-XX:MaxGCPauseMillis=200".to_string(),
            "-XX:+DisableExplicitGC".to_string(),
            "-XX:G1NewSizePercent=20".to_string(),
            "-XX:G1ReservePercent=20".to_string(),
            "-XX:G1HeapRegionSize=32M".to_string(),
        ]);
    }
    // Skip redundant DataFixerUpper rule optimization on startup so the game window opens much faster
    args.push("-Dminecraft.disableDataFixerOptimizations=true".to_string());
    args.push("-Dfml.readTimeout=90".to_string());
    args.extend(user_jvm_args);
    #[cfg(test)]
    if let Ok(extra) = std::env::var("IKA_BENCH_JVM") {
        args.extend(split_args(&extra));
    }
    args.push(
        "-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump"
            .to_string(),
    );
    let mem = |k: &str, d: u32| cfg.get(k).and_then(|v| v.parse::<u32>().ok()).unwrap_or(d);
    let (min_mem, max_mem) = if cfg.get("OverrideMemory").map_or(false, |v| v == "true") {
        (mem("MinMemAlloc", 512), mem("MaxMemAlloc", 8192))
    } else {
        (512, 8192)
    };
    args.push(format!("-Xms{}m", min_mem.min(max_mem)));
    args.push(format!("-Xmx{}m", min_mem.max(max_mem)));
    if !profile.jar_mods.is_empty() {
        // Same as Prism: old FML refuses a minecraft.jar that jar mods have modified.
        args.push("-Dfml.ignoreInvalidMinecraftCertificates=true".to_string());
        args.push("-Dfml.ignorePatchDiscrepancies=true".to_string());
    }
    args.extend(profile.jvm_args.iter().map(|a| replace_tokens(a, &tokens)));
    if let Some(arg) = boost_agent(root, &inst_dir, &gdir, &cfg, &profile, &java, &cp) {
        args.push(arg);
    }
    if let Some(arg) = skin_agent {
        args.push(arg);
        args.push("-Dauthlibinjector.noShowServerName".into());
    }
    args.push(format!(
        "-Djava.library.path={}",
        natives_dir.to_string_lossy()
    ));
    args.push("-Dminecraft.launcher.brand=IkaLauncher".to_string());
    args.push(format!(
        "-Dminecraft.launcher.version={}",
        env!("CARGO_PKG_VERSION")
    ));
    if let Some(a) = logging_arg {
        args.push(a);
    }
    if profile.main_class == FORGEWRAPPER_MAIN {
        // ForgeWrapper runs the Forge/NeoForge installer processors on first launch.
        let installer = profile
            .maven_files
            .iter()
            .find(|f| {
                f.get("name")
                    .and_then(|n| n.as_str())
                    .map_or(false, |n| n.ends_with(":installer"))
            })
            .and_then(|f| library_download(f, &libs_dir, None))
            .ok_or("No se encontró el instalador de Forge en los metadatos")?;
        args.push(format!(
            "-Dforgewrapper.librariesDir={}",
            libs_dir.to_string_lossy()
        ));
        args.push(format!(
            "-Dforgewrapper.installer={}",
            installer.path.to_string_lossy()
        ));
        args.push(format!(
            "-Dforgewrapper.minecraft={}",
            main_jar.path.to_string_lossy()
        ));
    }
    args.push("-cp".to_string());
    args.push(cp.join(sep));
    args.push(profile.main_class.clone());
    for a in profile.minecraft_arguments.split_whitespace() {
        args.push(replace_tokens(a, &tokens));
    }
    for t in &profile.tweakers {
        args.push("--tweakClass".to_string());
        args.push(t.clone());
    }
    // Prism's "join server on launch" (instance.cfg JoinServerOnLaunch / ...Address):
    // quick play on versions that have it, --server/--port on older ones.
    if cfg.get("JoinServerOnLaunch").map_or(false, |v| v == "true") {
        if let Some(addr) = cfg
            .get("JoinServerOnLaunchAddress")
            .map(|a| a.trim())
            .filter(|a| !a.is_empty())
        {
            let (host, port) = match addr.rsplit_once(':') {
                Some((h, p)) if p.parse::<u16>().is_ok() => (h.to_string(), p.to_string()),
                _ => (addr.to_string(), "25565".to_string()),
            };
            if profile.traits.contains("feature:is_quick_play_multiplayer") {
                args.push("--quickPlayMultiplayer".to_string());
                args.push(format!("{}:{}", host, port));
            } else {
                args.push("--server".to_string());
                args.push(host);
                args.push("--port".to_string());
                args.push(port);
            }
        }
    }

    // 7. Start. java.exe without a console window so its output can go to a log file
    // (the game's own logs/latest.log is written by Minecraft itself).
    let java_exe = {
        let c = java.with_file_name("java.exe");
        if c.exists() { c } else { java.clone() }
    };
    let logs_dir = gdir.join("logs");
    let _ = std::fs::create_dir_all(&logs_dir);
    let log_path = logs_dir.join("ika-launcher-output.log");
    let out_log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
    let err_log = out_log.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(&java_exe);
    cmd.args(&args)
        .current_dir(&gdir)
        .stdout(std::process::Stdio::from(out_log))
        .stderr(std::process::Stdio::from(err_log))
        .stdin(std::process::Stdio::null());
    // Minecraft 26.x probes Vulkan before it picks OpenGL. That probe loads every
    // system-wide implicit Vulkan layer (Overwolf, OBS, Steam, ReShade, EOS...);
    // with ten of them installed it added ~25 s before the window appeared.
    // Overlays still hook OpenGL on their own. The hybrid-GPU selection layers stay
    // enabled. Set IkaVulkanLayers=true in instance.cfg, or VK_LOADER_LAYERS_* in the
    // environment, to keep the usual behaviour.
    if cfg.get("IkaVulkanLayers").map_or(true, |v| v != "true")
        && std::env::var_os("VK_LOADER_LAYERS_DISABLE").is_none()
        && std::env::var_os("VK_LOADER_LAYERS_ENABLE").is_none()
    {
        cmd.env("VK_LOADER_LAYERS_DISABLE", "~implicit~").env(
            "VK_LOADER_LAYERS_ENABLE",
            "VK_LAYER_NV_optimus,VK_LAYER_AMD_switchable_graphics*",
        );
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let prewarming = PREWARM.with(|p| p.get());
    if prewarming {
        // The JVM option must come before the main class: rebuild the command.
        let mut all = vec!["-Dika.boost.prewarm=true".to_string()];
        all.extend(args.iter().cloned());
        let mut c = std::process::Command::new(&java_exe);
        c.args(&all).current_dir(&gdir).stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .env("VK_LOADER_LAYERS_DISABLE", "~implicit~");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS
            c.creation_flags(0x08000000 | 0x00004000);
        }
        let desktop = {
            #[cfg(windows)] { hidden_desktop() }
            #[cfg(not(windows))] { None::<String> }
        };
        let mut child = spawn_on_desktop(c, desktop.as_deref())
            .map_err(|e| format!("No se pudo preparar Minecraft: {e}"))?;
        let deadline = std::time::Instant::now() + Duration::from_secs(600);
        PREWARM_CANCEL.store(false, Ordering::SeqCst);
        loop {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() { break; }
            if PREWARM_CANCEL.load(Ordering::SeqCst) {
                let _ = child.kill();
                return Err("Preparación cancelada".into());
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                return Err("La preparación tardó demasiado".into());
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        return Ok(());
    }
    progress.emit("launch", "Iniciando Minecraft...", 0, 0);
    let started_at = std::time::SystemTime::now();
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("No se pudo iniciar Java ({}): {}", java_exe.display(), e))?;
    spawned(child.id());
    // Java can be alive for several seconds while Minecraft is still loading.
    // The dock enters “EN JUEGO” only after the actual game window exists.
    if wait_for_game_window(&mut child) {
        started();
    }
    let status = child
        .wait()
        .map_err(|e| format!("No se pudo esperar a Minecraft: {e}"))?;
    let report = find_crash_report(&gdir, started_at);
    if !status.success() || report.is_some() {
        let summary = report
            .as_deref()
            .and_then(crash_report_summary)
            .unwrap_or_else(|| output_error_tail(&log_path));
        return Err(format!(
            "Minecraft se cerró (código {}).\n{}",
            status.code().unwrap_or(-1),
            summary
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn wait_for_game_window(child: &mut std::process::Child) -> bool {
    use windows::{
        core::BOOL,
        Win32::{
            Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible},
        },
    };

    struct Search {
        pid: u32,
        found: bool,
    }
    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
        unsafe {
            let search = &mut *(data.0 as *mut Search);
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == search.pid && IsWindowVisible(hwnd).as_bool() {
                search.found = true;
                return BOOL(0);
            }
        }
        BOOL(1)
    }

    let pid = child.id();
    loop {
        let mut search = Search { pid, found: false };
        unsafe {
            let _ = EnumWindows(Some(visit), LPARAM((&mut search as *mut Search) as isize));
        }
        if search.found {
            return true;
        }
        if child.try_wait().ok().flatten().is_some() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(not(windows))]
fn wait_for_game_window(_child: &mut std::process::Child) -> bool {
    true
}
/// Maps a loader name or Prism UID to `(uid, cached_name, canonical_loader_name)`.
pub fn loader_uid_and_name(loader: &str) -> Option<(&'static str, &'static str, &'static str)> {
    let l = loader.trim().to_lowercase();
    if l.is_empty() || l == "vanilla" || l == "net.minecraft" {
        return None;
    }
    if l.contains("neoforge") || l == "net.neoforged" {
        Some(("net.neoforged", "NeoForge", "NeoForge"))
    } else if l.contains("forge") || l == "net.minecraftforge" {
        Some(("net.minecraftforge", "Forge", "Forge"))
    } else if l.contains("fabric") || l == "net.fabricmc.fabric-loader" {
        Some(("net.fabricmc.fabric-loader", "Fabric Loader", "Fabric"))
    } else if l.contains("quilt") || l == "org.quiltmc.quilt-loader" {
        Some(("org.quiltmc.quilt-loader", "Quilt Loader", "Quilt"))
    } else {
        None
    }
}

/// Detects the mod loader by inspecting jars in `<game_dir>/mods`.
pub fn detect_loader_from_mods(gdir: &Path) -> Option<String> {
    let mods_dir = gdir.join("mods");
    let entries = std::fs::read_dir(&mods_dir).ok()?;
    for entry in entries.flatten().take(12) {
        let p = entry.path();
        if !p
            .extension()
            .map_or(false, |e| e.eq_ignore_ascii_case("jar"))
        {
            continue;
        }
        let Ok(file) = std::fs::File::open(&p) else {
            continue;
        };
        let Ok(mut archive) = zip::ZipArchive::new(file) else {
            continue;
        };
        if archive.by_name("quilt.mod.json").is_ok() {
            return Some("Quilt".to_string());
        }
        if archive.by_name("fabric.mod.json").is_ok() {
            return Some("Fabric".to_string());
        }
        if archive.by_name("META-INF/neoforge.mods.toml").is_ok() {
            return Some("NeoForge".to_string());
        }
        if archive.by_name("META-INF/mods.toml").is_ok() || archive.by_name("mcmod.info").is_ok() {
            return Some("Forge".to_string());
        }
    }
    None
}

fn clean_loader_version_str(raw: &str, mc_version: &str) -> String {
    let mut s = raw.trim();
    for prefix in [
        "neoforge-",
        "forge-",
        "fabric-loader-",
        "fabric-",
        "quilt-loader-",
        "quilt-",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim();
        }
    }
    let mc_prefix = format!("{}-", mc_version);
    if let Some(rest) = s.strip_prefix(&mc_prefix) {
        s = rest.trim();
    }
    if let Some(idx) = s.find(&mc_prefix) {
        if idx == 0 {
            s = &s[mc_prefix.len()..];
        }
    }
    s.to_string()
}

pub fn fallback_loader_version(uid: &str, mc_version: &str) -> Option<String> {
    match uid {
        "net.fabricmc.fabric-loader" => Some("0.16.10".to_string()),
        "org.quiltmc.quilt-loader" => Some("0.27.0".to_string()),
        "net.neoforged" => match mc_version {
            "1.21.4" => Some("21.4.80-beta".to_string()),
            "1.21.1" | "1.21" => Some("21.1.77".to_string()),
            "1.20.6" => Some("20.6.119".to_string()),
            "1.20.4" => Some("20.4.237".to_string()),
            "1.20.2" => Some("20.2.88".to_string()),
            _ => Some("21.1.77".to_string()),
        },
        "net.minecraftforge" => match mc_version {
            "1.21.4" => Some("54.0.15".to_string()),
            "1.21.1" | "1.21" => Some("52.0.28".to_string()),
            "1.20.6" => Some("50.1.0".to_string()),
            "1.20.4" => Some("49.1.0".to_string()),
            "1.20.2" => Some("48.1.0".to_string()),
            "1.20.1" => Some("47.3.0".to_string()),
            "1.19.4" => Some("45.3.0".to_string()),
            "1.19.2" => Some("43.4.0".to_string()),
            "1.18.2" => Some("40.2.21".to_string()),
            "1.16.5" => Some("36.2.39".to_string()),
            "1.12.2" => Some("14.23.5.2860".to_string()),
            "1.7.10" => Some("10.13.4.1614".to_string()),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn resolve_loader_uid_version(
    agent: &ureq::Agent,
    uid: &str,
    mc_version: &str,
    preferred: Option<&str>,
) -> Option<String> {
    let clean_pref = preferred
        .map(|p| clean_loader_version_str(p, mc_version))
        .filter(|p| !p.is_empty());

    let url = format!("{}/{}/index.json", META_URL, uid);
    if let Ok(bytes) = http_get_bytes(agent, &url) {
        if let Ok(index) = serde_json::from_slice::<Value>(&bytes) {
            if let Some(versions) = index.get("versions").and_then(|v| v.as_array()) {
                let needs_mc_match = matches!(uid, "net.minecraftforge" | "net.neoforged");
                let matching: Vec<&Value> = if needs_mc_match {
                    versions
                        .iter()
                        .filter(|v| {
                            v.get("requires")
                                .and_then(|r| r.as_array())
                                .map_or(false, |reqs| {
                                    reqs.iter().any(|req| {
                                        req.get("uid").and_then(|u| u.as_str())
                                            == Some("net.minecraft")
                                            && req.get("equals").and_then(|e| e.as_str())
                                                == Some(mc_version)
                                    })
                                })
                        })
                        .collect()
                } else {
                    versions.iter().collect()
                };

                if let Some(pref) = &clean_pref {
                    if matching
                        .iter()
                        .any(|v| v.get("version").and_then(|s| s.as_str()) == Some(pref.as_str()))
                        || versions.iter().any(|v| {
                            v.get("version").and_then(|s| s.as_str()) == Some(pref.as_str())
                        })
                    {
                        return Some(pref.clone());
                    }
                }

                let chosen = matching
                    .iter()
                    .find(|v| v.get("recommended").and_then(|b| b.as_bool()) == Some(true))
                    .or_else(|| matching.first())
                    .and_then(|v| v.get("version").and_then(|s| s.as_str()));
                if let Some(ver) = chosen {
                    return Some(ver.to_string());
                }
            }
        }
    }

    clean_pref.or_else(|| fallback_loader_version(uid, mc_version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boost_default_only_for_forge_1_20_1() {
        let dir = tempfile::tempdir().unwrap();
        let pack = |mc: &str, loader: &str| format!(
            r#"{{"components":[{{"uid":"net.minecraft","version":"{mc}"}},{{"uid":"{loader}","version":"1"}}]}}"#);
        let check = |mc: &str, loader: &str, cfg: &str| {
            std::fs::write(dir.path().join("mmc-pack.json"), pack(mc, loader)).unwrap();
            std::fs::write(dir.path().join("instance.cfg"), cfg).unwrap();
            boost_enabled_dir(dir.path())
        };
        assert!(check("1.20.1", "net.minecraftforge", ""));
        assert!(!check("1.20.1", "net.minecraftforge", "IkaBoost=false\n"));
        assert!(!check("1.20.1", "net.fabricmc.fabric-loader", ""));
        assert!(!check("1.19.2", "net.minecraftforge", ""));
        assert!(check("1.19.2", "net.minecraftforge", "IkaBoost=true\n"));
    }

    #[test]
    #[ignore = "requires a JDK with javac and jar on PATH"]
    fn launch_with_local_prism_metadata() {
        launch_probe(None);
    }

    #[test]
    #[ignore = "requires a JDK and the existing authlib-injector cache"]
    fn launch_with_skin_injector() {
        let png = include_bytes!("../profile/steve.png").to_vec();
        launch_probe(Some(super::super::Skin { png, slim: true }));
    }

    fn launch_probe(skin: Option<super::super::Skin>) {
        // Exercise the production Java command with a minimal jar and cached files;
        // this uses no network and leaves installed games and saves untouched.
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let inst = root.join("Probe Instance");
        let classes = root.join("classes");
        let jar = root.join("libraries/test/probe/1/probe-1.jar");
        let assets = root.join("assets/indexes");
        for dir in [
            &classes,
            jar.parent().unwrap(),
            &assets,
            &inst.join("patches"),
        ] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(root.join("Probe.java"), r#"
public class Probe {
    public static void main(String[] args) throws Exception {
        System.out.println("probe-info");
        System.err.println("probe-error");
        java.nio.file.Files.writeString(java.nio.file.Path.of("probe.txt"), String.join("\n", args));
        for (String opt : java.lang.management.ManagementFactory.getRuntimeMXBean().getInputArguments()) {
            if (!opt.startsWith("-javaagent:") || !opt.contains("=http://127.0.0.1:")) continue;
            String server = opt.substring(opt.indexOf("=http://") + 1);
            String url = server + "sessionserver/session/minecraft/profile/" + args[3] + "?unsigned=false";
            java.net.http.HttpClient client = java.net.http.HttpClient.newHttpClient();
            var req = java.net.http.HttpRequest.newBuilder(java.net.URI.create(url)).build();
            var response = client.send(req, java.net.http.HttpResponse.BodyHandlers.ofString());
            if (response.statusCode() != 200) throw new AssertionError(response.statusCode());
            java.nio.file.Files.writeString(java.nio.file.Path.of("skin-profile.json"),response.body());
        }
    }
}"#).unwrap();
        let jdk = std::process::Command::new("where.exe")
            .arg("javac.exe")
            .output()
            .unwrap();
        assert!(jdk.status.success(), "javac no está en PATH");
        let jdk = String::from_utf8(jdk.stdout).unwrap();
        let javac = Path::new(jdk.lines().next().unwrap().trim());
        assert!(
            std::process::Command::new(javac)
                .args(["--release", "11", "-d"])
                .arg(&classes)
                .arg(root.join("Probe.java"))
                .status()
                .unwrap()
                .success()
        );
        assert!(
            std::process::Command::new(javac.with_file_name("jar.exe"))
                .args(["--create", "--file"])
                .arg(&jar)
                .arg("-C")
                .arg(&classes)
                .arg(".")
                .status()
                .unwrap()
                .success()
        );
        let cfg = format!(
            "name=Probe\nOverrideJavaLocation=true\nJavaPath={}\nOverrideMemory=true\nMinMemAlloc=64\nMaxMemAlloc=128\n",
            javac.with_file_name("java.exe").display()
        );
        std::fs::write(inst.join("instance.cfg"), cfg).unwrap();
        std::fs::write(
            inst.join("mmc-pack.json"),
            r#"{"components":[{"uid":"net.minecraft","version":"probe"}]}"#,
        )
        .unwrap();
        std::fs::write(inst.join("patches/net.minecraft.json"), r#"{
            "mainClass":"Probe",
            "mainJar":{"name":"test:probe:1"},
            "assetIndex":{"id":"probe"},
            "minecraftArguments":"--username ${auth_player_name} --uuid ${auth_uuid} --gameDir ${game_directory} --userType ${user_type}"
        }"#).unwrap();
        std::fs::write(assets.join("probe.json"), "{\"objects\":{}}").unwrap();

        if skin.is_some() {
            for name in ["authlib-injector.jar", "authlib-injector.sha256"] {
                std::fs::copy(ika_root().join(name), root.join(name)).unwrap();
            }
        }
        let user_type = if skin.is_some() { "mojang" } else { "legacy" };
        let mut started = false;
        let pid = std::cell::Cell::new(0);
        run_in(&root, &|_, _, _| {}, &inst, "IkaTest", skin.clone(), |child| pid.set(child), || {
            assert_ne!(pid.get(), 0);
            started = true;
        }).unwrap();
        assert_ne!(pid.get(), 0);
        // The probe emits logs and exits without creating a Minecraft window.
        assert!(!started);
        let output = std::fs::read_to_string(inst.join(".minecraft/logs/ika-launcher-output.log")).unwrap();
        assert!(output.contains("probe-info") && output.contains("probe-error"), "{output}");
        assert_eq!(
            std::fs::read_to_string(inst.join(".minecraft/probe.txt")).unwrap(),
            format!(
                "--username\nIkaTest\n--uuid\n{}\n--gameDir\n{}\n--userType\n{user_type}",
                offline_uuid("IkaTest"),
                inst.join(".minecraft").display()
            )
        );
        if let Some(skin) = skin {
            use base64::Engine;
            let path=inst.join(".minecraft/skin-profile.json");
            let info:Value=serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            assert_eq!(info["name"],"IkaTest");
            let property=&info["properties"][0];
            assert!(property["signature"].as_str().is_some_and(|s|!s.is_empty()));
            let bytes=base64::engine::general_purpose::STANDARD.decode(property["value"].as_str().unwrap()).unwrap();
            let textures:Value=serde_json::from_slice(&bytes).unwrap();
            assert_eq!(textures["textures"]["SKIN"]["metadata"]["model"],"slim");
            let png=http_get_bytes(&agent(),textures["textures"]["SKIN"]["url"].as_str().unwrap()).unwrap();
            assert_eq!(png,skin.png);
        }
    }
}

#[cfg(test)]
mod launch_bench {
    use super::*;
    use std::time::Instant;

#[test]
    #[ignore = "writes the boost fingerprint of an installed instance"]
    fn boost_fingerprint() {
        let name = std::env::var("IKA_BENCH").expect("IKA_BENCH");
        let inst = instances_root().join(&name);
        let gdir = game_dir(&inst);
        let cfg = read_cfg(&inst);
        let agent = agent();
        let mut profile = Profile::default();
        for (comp, meta) in &resolve_components(&agent, &inst).unwrap() {
            profile.apply(comp, meta);
        }
        let java = PathBuf::from(cfg.get("JavaPath").unwrap());
        println!("{:?}", boost_agent(&ika_root(), &inst, &gdir, &cfg, &profile, &java, &[]));
    }

    #[test]
    #[ignore = "warms the IkaBoost cache of an installed instance (hidden desktop)"]
    fn boost_prewarm() {
        let name = std::env::var("IKA_BENCH").expect("IKA_BENCH");
        let t = Instant::now();
        prewarm(&instances_root().join(&name), "Faally").unwrap();
        println!("PREWARM {name}: {:?}", t.elapsed());
    }

    /// IKA_BENCH=<instance folder name>. Measures prep, window and title-screen times.
    #[test]
    #[ignore = "launches a real installed instance"]
    fn bench_launch() {
        let name = std::env::var("IKA_BENCH").expect("IKA_BENCH");
        let inst = instances_root().join(&name);
        let gdir = game_dir(&inst);
        let log = gdir.join("logs").join("latest.log");
        struct Restore(PathBuf, String);
        impl Drop for Restore { fn drop(&mut self) { let _ = std::fs::write(&self.0, &self.1); } }
        let cfg_path = inst.join("instance.cfg");
        let original = std::fs::read_to_string(&cfg_path).unwrap();
        let _restore = Restore(cfg_path.clone(), original.clone());
        if let Ok(pairs) = std::env::var("IKA_BENCH_CFG") {
            let values: Vec<(&str, String)> = pairs.split(';').filter_map(|p| p.split_once('=')).map(|(k, v)| (k, v.to_string())).collect();
            set_cfg_values(&inst, &values);
        }
        let t0 = Instant::now();
        let pid = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let spawn_at = std::sync::Arc::new(Mutex::new(None::<Duration>));
        let window_at = std::sync::Arc::new(Mutex::new(None::<Duration>));
        let (p2, s2) = (pid.clone(), spawn_at.clone());
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let watcher = {
            let pid = pid.clone();
            let log = log.clone();
            let done = done.clone();
            std::thread::spawn(move || {
                while pid.load(Ordering::SeqCst) == 0 { if done.load(Ordering::SeqCst) { panic!("no game process"); } std::thread::sleep(Duration::from_millis(10)); }
                let start = std::time::SystemTime::now();
                let mut last_len = 0u64;
                let mut last_change = Instant::now();
                let mut ready: Option<Duration> = None; let mut last_atlases = 0usize; let mut last_atlas_at = Duration::ZERO;
                loop {
                    std::thread::sleep(Duration::from_millis(100));
                    if done.load(Ordering::SeqCst) { panic!("game exited"); }
                    let Ok(m) = std::fs::metadata(&log) else { continue };
                    if m.modified().map_or(true, |t| t < start) { continue; }
                    if m.len() != last_len { last_len = m.len(); last_change = Instant::now(); }
                    let text = String::from_utf8_lossy(&std::fs::read(&log).unwrap_or_default()).into_owned();
                    let atlases = text.matches("-atlas").count();
                    if atlases > 0 && ready.is_none() { ready = Some(t0.elapsed()); }
                    if atlases != last_atlases { last_atlases = atlases; last_atlas_at = t0.elapsed(); }
                    if ready.is_some() && (last_change.elapsed() > Duration::from_secs(5) || t0.elapsed() - ready.unwrap() > Duration::from_secs(20)) {
                        let at = last_atlas_at;
                        let p = pid.load(Ordering::SeqCst).to_string();
                        let _ = std::process::Command::new("taskkill").args(["/PID",&p]).output();
                        std::thread::spawn(move || { std::thread::sleep(Duration::from_secs(40)); let _ = std::process::Command::new("taskkill").args(["/F","/T","/PID",&p]).output(); });
                        return at;
                    }
                    if t0.elapsed() > Duration::from_secs(240) || done.load(Ordering::SeqCst) {
                        let _ = std::process::Command::new("taskkill").args(["/F","/T","/PID",&pid.load(Ordering::SeqCst).to_string()]).output();
                        panic!("game exited or timeout");
                    }
                }
            })
        };
        let w2 = window_at.clone();
        let result = run_in(&ika_root(), &|m, _, _| eprintln!("[{:>6} ms] {m}", t0.elapsed().as_millis()), &inst, "Faally", None,
            move |p| { *s2.lock().unwrap() = Some(t0.elapsed()); p2.store(p, Ordering::SeqCst); },
            move || { *w2.lock().unwrap() = Some(t0.elapsed()); });
        done.store(true, Ordering::SeqCst);
        if let Err(e) = &result { eprintln!("run_in: {e}"); }
        let ready = watcher.join().unwrap();
        println!("BENCH {name}: spawn={:?} window={:?} ready={:?}", spawn_at.lock().unwrap().unwrap(), *window_at.lock().unwrap(), ready);
        // Phase times (s since Play) from latest.log's HH:MM:SS stamps.
        let text = String::from_utf8_lossy(&std::fs::read(&log).unwrap_or_default()).into_owned();
        let secs = |l: &str| -> Option<i64> {
            let h: i64 = l.get(1..3)?.parse().ok()?; let m: i64 = l.get(4..6)?.parse().ok()?; let s: i64 = l.get(7..9)?.parse().ok()?;
            Some(h * 3600 + m * 60 + s)
        };
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        let wall_now = { let out = std::process::Command::new("powershell").args(["-NoProfile","-Command","(Get-Date).ToString('HH:mm:ss')"]).output().unwrap(); let s = String::from_utf8_lossy(&out.stdout).trim().to_string(); secs(&format!("[{s}]")).unwrap() };
        let elapsed_now = t0.elapsed().as_secs() as i64;
        let t0_wall = wall_now - elapsed_now;
        let _ = now;
        let mut out = String::new();
        for (label, pat) in [("modlauncher", "ModLauncher running"), ("window", "Loading ImmediateWindowProvider"), ("target", "Launching target"), ("lwjgl", "Backend library"), ("resources", "Reloading ResourceManager"), ("sound", "Sound engine started")] {
            if let Some(l) = text.lines().find(|l| l.contains(pat)) {
                if let Some(s) = secs(l) { out.push_str(&format!(" {label}={}", s - t0_wall)); }
            }
        }
        if let Some(s) = text.lines().filter(|l| l.contains("-atlas")).last().and_then(|l| secs(l)) { out.push_str(&format!(" atlas={}", s - t0_wall)); }
        println!("PHASES{out}");
    }
}
