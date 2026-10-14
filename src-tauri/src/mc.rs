//! Standalone Minecraft installer + offline launcher.
//!
//! Works like Prism Launcher / PineconeMC without needing either installed: instances use
//! Prism's format (`instance.cfg` + `mmc-pack.json`), components are resolved from the
//! public Prism meta (`meta.prismlauncher.org`, which also has Forge/NeoForge ready to run
//! through ForgeWrapper), every file is downloaded and SHA-1 checked into the launcher's own
//! data dir (`%APPDATA%\IkaLauncher`), and the game is started directly with an offline
//! account (Prism's name-based UUID).

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use tauri::Emitter;

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
    ika_root().join("instances")
}

/// Instances currently being prepared/launched, so a double click doesn't start two.
static BUSY: Mutex<Option<HashSet<String>>> = Mutex::new(None);

struct BusyGuard(String);
impl Drop for BusyGuard {
    fn drop(&mut self) {
        if let Some(set) = BUSY.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            set.remove(&self.0);
        }
    }
}

fn claim_instance(key: &str) -> Result<BusyGuard, String> {
    let mut lock = BUSY.lock().unwrap_or_else(|e| e.into_inner());
    let set = lock.get_or_insert_with(HashSet::new);
    if !set.insert(key.to_string()) {
        return Err("Esta instancia ya se está preparando.".to_string());
    }
    Ok(BusyGuard(key.to_string()))
}

// ---------------------------------------------------------------------------------------
// Progress
// ---------------------------------------------------------------------------------------

/// Sends `mc-launch-progress` events to the UI (`app` is None in tests).
#[derive(Clone)]
pub(crate) struct Progress {
    pub(crate) app: Option<tauri::AppHandle>,
    pub(crate) instance: String,
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

impl Progress {
    fn emit(&self, stage: &str, message: &str, done: usize, total: usize) {
        self.emit_with_speed(stage, message, done, total, None);
    }

    fn emit_with_speed(&self, stage: &str, message: &str, done: usize, total: usize, speed: Option<&str>) {
        let Some(app) = &self.app else { return };
        let mut obj = serde_json::json!({
            "instance": self.instance,
            "stage": stage,
            "message": message,
            "done": done,
            "total": total,
        });
        if let Some(s) = speed {
            obj["speed"] = serde_json::Value::String(s.to_string());
        }
        let _ = app.emit("mc-launch-progress", obj);
    }
}

// ---------------------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------------------

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout_read(std::time::Duration::from_secs(60))
        .user_agent(USER_AGENT)
        .build()
}

fn http_get_bytes(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, String> {
    let resp = agent.get(url).call().map_err(|e| format!("{}: {}", url, e))?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf).map_err(|e| format!("{}: {}", url, e))?;
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
                    std::fs::write(&tmp, &data).map_err(|e| format!("{}: {}", self.path.display(), e))?;
                    std::fs::rename(&tmp, &self.path).map_err(|e| format!("{}: {}", self.path.display(), e))?;
                    return Ok(len);
                }
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }
}

/// Downloads every missing file in parallel, reporting progress under `stage`.
fn download_all(list: Vec<Download>, progress: &Progress, stage: &str, label: &str) -> Result<(), String> {
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
                    if speed_str.is_empty() { None } else { Some(&speed_str) },
                );
            }
        });
    });
    let errors = errors.into_inner().unwrap_or_default();
    if !errors.is_empty() {
        return Err(format!(
            "No se pudieron descargar {} archivo(s). Revisa tu conexión.\n{}",
            errors.len(),
            errors.iter().take(3).cloned().collect::<Vec<_>>().join("\n")
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
    let cache = ika_root().join("meta").join(uid).join(format!("{}.json", version));
    if let Ok(text) = std::fs::read_to_string(&cache) {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            return Ok(v);
        }
    }
    let url = format!("{}/{}/{}.json", META_URL, uid, version);
    let data = http_get_bytes(agent, &url)
        .map_err(|e| format!("No se pudo obtener {} {} (¿sin conexión?): {}", uid, version, e))?;
    let v: Value = serde_json::from_slice(&data).map_err(|e| format!("Metadatos inválidos de {}: {}", uid, e))?;
    if let Some(parent) = cache.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&cache, &data);
    Ok(v)
}

/// Mojang's official version file for `mc_version` (cached like the Prism meta).
fn load_mojang_version(agent: &ureq::Agent, mc_version: &str) -> Result<Value, String> {
    let cache = ika_root().join("meta").join("mojang").join(format!("{}.json", mc_version));
    if let Ok(v) = std::fs::read_to_string(&cache).map_err(|_| ()).and_then(|t| serde_json::from_str::<Value>(&t).map_err(|_| ())) {
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
        .and_then(|vs| vs.iter().find(|v| v.get("id").and_then(|i| i.as_str()) == Some(mc_version)))
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
fn resolve_components(agent: &ureq::Agent, inst_dir: &Path) -> Result<Vec<(Component, Value)>, String> {
    let pack_text = std::fs::read_to_string(inst_dir.join("mmc-pack.json"))
        .map_err(|_| "La instancia no tiene mmc-pack.json".to_string())?;
    let pack: Value = serde_json::from_str(&pack_text).map_err(|e| format!("mmc-pack.json inválido: {}", e))?;

    let mut comps: Vec<Component> = Vec::new();
    for c in pack.get("components").and_then(|c| c.as_array()).into_iter().flatten() {
        let uid = c.get("uid").and_then(|v| v.as_str()).unwrap_or("").to_string();
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
            "net.fabricmc.fabric-loader" | "org.quiltmc.quilt-loader" | "net.minecraftforge" | "net.neoforged"
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
            serde_json::from_str(&text).map_err(|e| format!("Parche {} inválido: {}", comp.uid, e))?
        } else {
            if comp.version.is_empty() {
                if let Some(resolved) = resolve_loader_uid_version(agent, &comp.uid, &mc_version, None) {
                    comp.version = resolved.clone();
                    comps[i].version = resolved;
                } else {
                    return Err(format!("El componente {} no tiene versión", comp.uid));
                }
            }
            load_meta(agent, &comp.uid, &comp.version)?
        };
        for req in meta.get("requires").and_then(|r| r.as_array()).into_iter().flatten() {
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
            comps.push(Component { uid: ruid.to_string(), version });
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
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

impl Profile {
    fn apply(&mut self, comp: &Component, m: &Value) {
        if comp.uid == "net.minecraft" {
            self.mc_version = comp.version.clone();
            self.version_type = m.get("type").and_then(|t| t.as_str()).unwrap_or("release").to_string();
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
        for f in m.get("mavenFiles").and_then(|l| l.as_array()).into_iter().flatten() {
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
            self.java_majors = majors.iter().filter_map(|x| x.as_u64().map(|n| n as u32)).collect();
        }
        if let Some(n) = m.get("compatibleJavaName").and_then(|v| v.as_str()) {
            self.java_name = Some(n.to_string());
        }
        for jm in m.get("jarMods").and_then(|j| j.as_array()).into_iter().flatten() {
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
    Some(format!("{}/{}/{}/{}", group.replace('.', "/"), artifact, version, file))
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
                let name_ok = os.get("name").and_then(|n| n.as_str()).map_or(true, |n| n == "windows");
                let arch_ok = os.get("arch").and_then(|a| a.as_str()).map_or(true, |a| a == "x86_64" || a == "x64");
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
        Some(c) => lib.pointer("/downloads/classifiers").and_then(|cl| cl.get(c)),
        None => lib.pointer("/downloads/artifact"),
    };
    let rel = artifact
        .and_then(|a| a.get("path").and_then(|p| p.as_str()).map(str::to_string))
        .or_else(|| maven_path(name, classifier))?;
    let path = libs_dir.join(rel.replace('/', "\\"));
    let url = match artifact.and_then(|a| a.get("url").and_then(|u| u.as_str())) {
        Some(u) => u.to_string(),
        None => {
            let base = lib.get("url").and_then(|u| u.as_str()).unwrap_or(MOJANG_LIBRARIES);
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

fn read_cfg(inst_dir: &Path) -> HashMap<String, String> {
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
fn game_dir(inst_dir: &Path) -> PathBuf {
    let dot = inst_dir.join(".minecraft");
    let plain = inst_dir.join("minecraft");
    if dot.is_dir() {
        dot
    } else if plain.is_dir() {
        plain
    } else if inst_dir.join("instance.json").is_file() || inst_dir.join("mods").is_dir() {
        inst_dir.to_path_buf()
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
fn unescape_ini(v: &str) -> String {
    let v = v.trim();
    let v = v.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(v);
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
fn skinrestorer_supports(mc_version: &str) -> bool {
    let mut parts = mc_version.split('.');
    match (parts.next().and_then(|p| p.parse::<u32>().ok()), parts.next().and_then(|p| p.parse::<u32>().ok())) {
        (Some(1), Some(minor)) => minor >= 19,
        (Some(major), _) => major >= 26,
        _ => false,
    }
}

/// Keeps SkinRestorer (with the Ika config) in a modded instance. Removing its jar from the
/// mods folder opts the instance out for good (IkaShareSkins=false). Never blocks a launch.
#[allow(clippy::too_many_arguments)]
fn share_skins_in_instance(
    agent: &ureq::Agent,
    inst_dir: &Path,
    gdir: &Path,
    cfg: &HashMap<String, String>,
    mc_version: &str,
    loader: &str,
    service: &str,
    progress: &Progress,
) {
    if cfg.get("IkaShareSkins").map_or(false, |v| v == "false") || !skinrestorer_supports(mc_version) {
        return;
    }
    let mods = gdir.join("mods");
    let existing = crate::skinrestorer_setup::has_skinrestorer(&mods);
    if existing.is_none() && cfg.get("IkaSkinRestorerJar").map_or(false, |j| !j.is_empty()) {
        // We installed it before and it's gone: the player removed it on purpose.
        set_cfg_values(inst_dir, &[("IkaShareSkins", "false".to_string())]);
        return;
    }
    let check_tag = format!("{}:{}", mc_version, loader);
    if existing.is_some() {
        let _ = crate::skinrestorer_setup::merge_config(gdir, service);
        return;
    }
    if cfg.get("IkaSkinRestorerChecked").map_or(false, |v| v == &check_tag) {
        return;
    }
    let inst_dir_bg = inst_dir.to_path_buf();
    let gdir_bg = gdir.to_path_buf();
    let mc_bg = mc_version.to_string();
    let loader_bg = loader.to_string();
    let service_bg = service.to_string();
    let progress_bg = progress.clone();
    let agent_bg = agent.clone();
    std::thread::spawn(move || {
        let mods_bg = gdir_bg.join("mods");
        let res = crate::skinrestorer_setup::install_into(&agent_bg, &gdir_bg, &mc_bg, &loader_bg, &service_bg)
            .map(|_| crate::skinrestorer_setup::has_skinrestorer(&mods_bg));
        match res {
            Ok(Some(jar)) => {
                set_cfg_values(
                    &inst_dir_bg,
                    &[
                        ("IkaSkinRestorerJar", jar),
                        ("IkaSkinRestorerChecked", check_tag),
                    ],
                );
                progress_bg.emit(
                    "skins-lan",
                    "SkinRestorer añadido: al abrir tu mundo en LAN, tus amigos verán las skins (bórralo de la carpeta mods si no lo quieres)",
                    0,
                    0,
                );
            }
            Ok(None) => {
                set_cfg_values(&inst_dir_bg, &[("IkaSkinRestorerChecked", check_tag)]);
            }
            Err(_) => {
                set_cfg_values(&inst_dir_bg, &[("IkaSkinRestorerChecked", check_tag)]);
            }
        }
    });
}

/// Rewrites (or adds) keys of the instance's instance.cfg, keeping everything else.
fn set_cfg_values(inst_dir: &Path, values: &[(&str, String)]) {
    let path = inst_dir.join("instance.cfg");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    for (key, value) in values {
        let entry = format!("{}={}", key, value);
        match lines.iter().position(|l| l.split_once('=').map_or(false, |(k, _)| k.trim() == *key)) {
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
        if p.file_name().map_or(false, |n| n.eq_ignore_ascii_case("java.exe")) {
            let w = p.with_file_name("javaw.exe");
            if w.exists() {
                return w;
            }
        }
        p
    };
    let override_on = cfg.get("OverrideJavaLocation").map_or(false, |v| v == "true");
    if let Some(jp) = cfg.get("JavaPath").filter(|p| !p.is_empty()) {
        if override_on || cfg.get("OverrideJavaLocation").is_none() {
            let p = to_javaw(jp);
            if p.exists() {
                // If automatically cached (not explicit override), verify major version still matches profile
                let compatible = override_on
                    || profile.java_majors.is_empty()
                    || java_major_of(&p).map_or(true, |m| profile.java_majors.contains(&m));
                if compatible {
                    return Ok(p);
                }
            }
        }
    }

    // 2. Runtime chosen in the creation screen
    if let Some(vendor) = cfg.get("IkaJavaVendor").filter(|v| !v.is_empty()) {
        let major: u32 = cfg.get("IkaJavaMajor").and_then(|m| m.parse().ok()).unwrap_or(0);
        let runtime = cfg.get("IkaJavaRuntime").filter(|r| !r.is_empty()).cloned();
        let url = cfg.get("IkaJavaUrl").filter(|u| !u.is_empty()).cloned();
        let folder = crate::java_runtime_folder(vendor, major, runtime.as_deref());
        let appdata = PathBuf::from(std::env::var("APPDATA").unwrap_or_default());
        let existing = [
            crate::java_install_root(),
            appdata.join("ElyPrismLauncher").join("java"),
            appdata.join("PrismLauncher").join("java"),
        ]
        .into_iter()
        .map(|root| root.join(&folder).join("bin").join("javaw.exe"))
        .find(|p| p.exists());
        let javaw = match existing {
            Some(p) => p,
            None => {
                let label = runtime.clone().unwrap_or_else(|| format!("Java {}", major));
                progress.emit("java", &format!("Descargando {}...", label), 0, 0);
                let inst = crate::download_java_runtime_inner(vendor, major, url, runtime)?;
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

    // 3. Automatic (persist resolved JavaPath so subsequent launches skip full-disk Java scan)
    let wanted: Vec<u32> = if profile.java_majors.is_empty() { vec![8] } else { profile.java_majors.clone() };
    let installed = crate::list_java_installations();
    for major in &wanted {
        if let Some(j) = installed.iter().find(|j| j.major == *major && Path::new(&j.path).exists()) {
            let javaw = PathBuf::from(&j.path);
            set_cfg_values(
                inst_dir,
                &[("JavaPath", javaw.to_string_lossy().replace('\\', "/"))],
            );
            return Ok(javaw);
        }
    }

    let major = wanted[0];
    progress.emit("java", &format!("Descargando Java {} de Mojang...", major), 0, 0);
    let inst = crate::download_java_runtime_inner("mojang", major, None, profile.java_name.clone())?;
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
        stamp.push_str(&format!("{}|{}|{}\n", jar.display(), len, excludes.join(",")));
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
        let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("{}: {}", jar.display(), e))?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().to_string();
            if entry.is_dir() || name.starts_with("META-INF/") || excludes.iter().any(|x| name.starts_with(x.as_str())) {
                continue;
            }
            let Some(rel) = entry.enclosed_name() else { continue };
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
fn build_modded_jar(inst_dir: &Path, vanilla: &Path, jar_mods: &[String]) -> Result<PathBuf, String> {
    let mods: Vec<PathBuf> = jar_mods.iter().map(|f| inst_dir.join("jarmods").join(f)).collect();
    if let Some(missing) = mods.iter().find(|m| !m.exists()) {
        return Err(format!("Falta el jar mod {}", missing.display()));
    }
    let out = inst_dir.join("bin").join("minecraft.jar");
    // Reuse the previous build while none of its inputs changed.
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    if let Some(built) = modified(&out) {
        if mods.iter().chain(std::iter::once(&vanilla.to_path_buf())).all(|p| modified(p).map_or(false, |t| t <= built)) {
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
        let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("{}: {}", src.display(), e))?;
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

/// Skin chosen in the launcher profile, applied in game through authlib-injector.
pub struct SkinInput {
    pub png: Vec<u8>,
    pub slim: bool,
}

const AUTHLIB_INJECTOR_LATEST: &str = "https://authlib-injector.yushi.moe/artifact/latest.json";

/// authlib-injector jar (official build, SHA-256 checked), downloaded once.
fn ensure_authlib_injector(agent: &ureq::Agent) -> Result<PathBuf, String> {
    let jar = ika_root().join("authlib-injector.jar");
    let sum_file = ika_root().join("authlib-injector.sha256");
    if jar.metadata().map(|m| m.is_file() && m.len() > 50_000).unwrap_or(false) && sum_file.is_file() {
        return Ok(jar);
    }
    let sha256 = |data: &[u8]| {
        use sha2::Digest;
        sha2::Sha256::digest(data).iter().map(|b| format!("{:02x}", b)).collect::<String>()
    };
    if let (Ok(data), Ok(expected)) = (std::fs::read(&jar), std::fs::read_to_string(&sum_file)) {
        if sha256(&data) == expected.trim() {
            return Ok(jar);
        }
    }
    let meta: Value = serde_json::from_slice(&http_get_bytes(agent, AUTHLIB_INJECTOR_LATEST)?).map_err(|e| e.to_string())?;
    let url = meta.get("download_url").and_then(|v| v.as_str()).ok_or("authlib-injector: respuesta inválida")?;
    let expected = meta.pointer("/checksums/sha256").and_then(|v| v.as_str()).ok_or("authlib-injector: sin checksum")?;
    let data = http_get_bytes(agent, url)?;
    if sha256(&data) != expected.to_lowercase() {
        return Err("authlib-injector: el checksum no coincide".to_string());
    }
    std::fs::create_dir_all(ika_root()).map_err(|e| e.to_string())?;
    std::fs::write(&jar, &data).map_err(|e| e.to_string())?;
    std::fs::write(&sum_file, expected.to_lowercase()).map_err(|e| e.to_string())?;
    Ok(jar)
}

/// Games started from here that are still running: instance path (lowercase) -> (pid, start).
pub static RUNNING_GAMES: Mutex<Option<HashMap<String, (u32, std::time::Instant)>>> = Mutex::new(None);

/// Games the user asked to stop (console_tools::stop_game), by the same key: when one of them
/// exits, it is a close and not a crash.
pub static STOPPED_GAMES: Mutex<Option<HashSet<String>>> = Mutex::new(None);

pub(crate) struct Started {
    pub(crate) child: std::process::Child,
    pub(crate) output_log: PathBuf,
    pub(crate) mc_version: String,
    pub(crate) game_dir: PathBuf,
    pub(crate) started_at: std::time::SystemTime,
    /// Threads copying the game's output to the log file and the UI.
    pub(crate) output_threads: Vec<std::thread::JoinHandle<()>>,
}

pub fn launch_instance(
    app: tauri::AppHandle,
    inst_path: String,
    username: String,
    skin: Option<SkinInput>,
) -> Result<String, String> {
    let username = username.trim().to_string();
    if username.is_empty() {
        return Err("Agrega un usuario en tu perfil para poder jugar.".to_string());
    }
    // The UI may hand over the instance's mods folder: use the folder that has mmc-pack.json.
    let inst_path = Path::new(&inst_path)
        .ancestors()
        .take(4)
        .find(|p| p.join("mmc-pack.json").exists())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(inst_path);
    let _busy = claim_instance(&inst_path.to_lowercase())?;
    let progress = Progress { app: Some(app), instance: inst_path.clone() };
    let result = prepare_and_spawn(&progress, Path::new(&inst_path), &username, skin);
    let mut started = match result {
        Ok(s) => s,
        Err(e) => {
            progress.emit("error", &e, 0, 0);
            return Err(e);
        }
    };

    // Watch the game until it closes: the UI gets "mc-game-exit" with the exit code and, if
    // it crashed, the cause taken from the crash report.
    let (exit_tx, exit_rx) = std::sync::mpsc::channel::<String>();
    let mc_version = started.mc_version.clone();
    let watch_progress = progress.clone();
    let running_key = inst_path.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    RUNNING_GAMES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(running_key.clone(), (started.child.id(), std::time::Instant::now()));
    std::thread::spawn(move || {
        let status = started.child.wait();
        if let Some(map) = RUNNING_GAMES.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            map.remove(&running_key);
        }
        // Let the output threads flush the last lines before reporting.
        for t in started.output_threads.drain(..) {
            let _ = t.join();
        }
        let stopped = STOPPED_GAMES.lock().unwrap_or_else(|e| e.into_inner()).as_mut().is_some_and(|set| set.remove(&running_key));
        let code = status.ok().and_then(|s| s.code()).unwrap_or(-1);
        let report = find_crash_report(&started.game_dir, started.started_at);
        let crashed = !stopped && (code != 0 || report.is_some());
        let summary = if crashed {
            report
                .as_ref()
                .and_then(|r| crash_report_summary(r))
                .unwrap_or_else(|| output_error_tail(&started.output_log))
        } else {
            String::new()
        };
        if let Some(app) = &watch_progress.app {
            let _ = app.emit(
                "mc-game-exit",
                serde_json::json!({
                    "instance": watch_progress.instance,
                    "code": code,
                    "crashed": crashed,
                    "stopped": stopped,
                    "summary": summary,
                    "crashReport": report.as_ref().map(|r| r.to_string_lossy().to_string()),
                }),
            );
        }
        let _ = exit_tx.send(if crashed {
            format!("Minecraft se cerró (código {}).\n{}", code, summary)
        } else {
            String::new()
        });
    });

    // Crashes during immediate JVM startup (bad Java args, missing DLL...) show up within a few hundred ms;
    // any later crash is reported asynchronously via the "mc-game-exit" event.
    if let Ok(msg) = exit_rx.recv_timeout(std::time::Duration::from_millis(450)) {
        if !msg.is_empty() {
            progress.emit("error", &msg, 0, 0);
            return Err(msg);
        }
    }
    progress.emit("done", "Minecraft iniciado", 1, 1);
    Ok(format!("Minecraft {} iniciado como {}", mc_version, username))
}

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
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim_start().starts_with('<')).collect();
    let interesting: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| {
            let lower = l.to_lowercase();
            lower.contains("error") || lower.contains("exception") || lower.contains("caused by") || lower.starts_with("\tat ")
        })
        .collect();
    let pick = if interesting.is_empty() { &lines } else { &interesting };
    pick[pick.len().saturating_sub(10)..].join("\n")
}

// ---------------------------------------------------------------------------------------
// Game output -> log file + UI console
// ---------------------------------------------------------------------------------------

/// One console line for the UI.
#[derive(serde::Serialize, Clone)]
pub(crate) struct ConsoleLine {
    pub(crate) level: String,
    pub(crate) text: String,
    /// Epoch ms from the log4j event (0 when unknown); the UI shows it in local time.
    ts: i64,
}

/// Turns the game's console output into readable lines. Minecraft prints log4j XML events
/// (`<log4j:Event level=.. thread=..>` with the message in CDATA, possibly over several
/// lines); anything else (stderr, old versions) is passed through with a guessed level.
#[derive(Default)]
pub(crate) struct Log4jParser {
    in_event: bool,
    level: String,
    thread: String,
    ts: i64,
    buf: String,
}

fn xml_attr(tag: &str, name: &str) -> String {
    let key = format!("{}=\"", name);
    tag.find(&key)
        .and_then(|i| tag[i + key.len()..].split('"').next())
        .unwrap_or("")
        .to_string()
}

fn guess_level(line: &str) -> &'static str {
    let lower = line.to_lowercase();
    if lower.contains("error") || lower.contains("exception") || lower.contains("caused by") || line.starts_with("\tat ") || lower.contains("fatal") {
        "ERROR"
    } else if lower.contains("warn") {
        "WARN"
    } else {
        "INFO"
    }
}

impl Log4jParser {
    pub(crate) fn feed(&mut self, line: &str, out: &mut Vec<ConsoleLine>) {
        let trimmed = line.trim();
        if !self.in_event {
            if let Some(rest) = trimmed.strip_prefix("<log4j:Event") {
                self.in_event = true;
                self.level = xml_attr(rest, "level");
                self.thread = xml_attr(rest, "thread");
                self.ts = xml_attr(rest, "timestamp").parse().unwrap_or(0);
                self.buf.clear();
                return;
            }
            if !trimmed.is_empty() {
                out.push(ConsoleLine { level: guess_level(line).to_string(), text: line.trim_end().to_string(), ts: 0 });
            }
            return;
        }
        if trimmed.starts_with("</log4j:Event>") {
            self.in_event = false;
            let level = if self.level.is_empty() { "INFO".to_string() } else { self.level.clone() };
            let mut first = true;
            for msg_line in self.buf.lines() {
                if msg_line.trim().is_empty() {
                    continue;
                }
                let text = if first {
                    format!("[{}/{}] {}", self.thread, level, msg_line)
                } else {
                    msg_line.to_string()
                };
                first = false;
                out.push(ConsoleLine { level: level.clone(), text, ts: self.ts });
            }
            return;
        }
        // Inside an event: keep the CDATA contents (message and throwable).
        // A line that opens a tag carries the XML indentation before it: drop that.
        let mut l = if trimmed.starts_with("<log4j:") { trimmed.to_string() } else { line.to_string() };
        for tag in ["<log4j:Message>", "</log4j:Message>", "<log4j:Throwable>", "</log4j:Throwable>", "<![CDATA[", "]]>"] {
            l = l.replace(tag, "");
        }
        if !l.trim().is_empty() {
            self.buf.push_str(l.trim_end());
            self.buf.push('\n');
        }
    }
}

/// Copies one output pipe of the game to the log file and sends it to the UI in batches.
fn spawn_output_reader<R: std::io::Read + Send + 'static>(
    pipe: R,
    log: std::sync::Arc<Mutex<std::fs::File>>,
    app: Option<tauri::AppHandle>,
    instance: String,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        use std::io::{BufRead, Write};
        let mut reader = std::io::BufReader::new(pipe);
        let mut parser = Log4jParser::default();
        let mut pending: Vec<ConsoleLine> = Vec::new();
        let mut last_emit = std::time::Instant::now();
        let mut raw = Vec::new();
        let flush = |pending: &mut Vec<ConsoleLine>| {
            if pending.is_empty() {
                return;
            }
            if let Some(app) = &app {
                let _ = app.emit("mc-game-log", serde_json::json!({ "instance": instance, "lines": pending }));
            }
            pending.clear();
        };
        loop {
            raw.clear();
            match reader.read_until(b'\n', &mut raw) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if let Ok(mut f) = log.lock() {
                let _ = f.write_all(&raw);
            }
            let line = String::from_utf8_lossy(&raw);
            parser.feed(line.trim_end_matches(['\r', '\n']), &mut pending);
            if pending.len() >= 200 || last_emit.elapsed().as_millis() >= 150 {
                flush(&mut pending);
                last_emit = std::time::Instant::now();
            }
        }
        flush(&mut pending);
    })
}

/// Pre-downloads and prepares everything an instance needs (Minecraft jar, Modloader libraries,
/// assets, Java runtime, natives, SkinRestorer, authlib-injector) without launching the game,
/// so that when a received instance is finished, clicking Play launches immediately with zero second download.
pub fn preinstall_instance_runtime(
    app: Option<tauri::AppHandle>,
    inst_dir: &Path,
) -> Result<(), String> {
    if !inst_dir.join("mmc-pack.json").exists() {
        return Ok(());
    }
    let progress = Progress {
        app,
        instance: inst_dir.to_string_lossy().to_string(),
    };
    let agent = agent();
    let root = ika_root();
    let libs_dir = root.join("libraries");
    let assets_dir = root.join("assets");
    let cfg = read_cfg(inst_dir);
    let gdir = game_dir(inst_dir);
    std::fs::create_dir_all(&gdir).map_err(|e| e.to_string())?;

    progress.emit("meta", "Preparando versión de Minecraft y Loader...", 0, 0);
    let components = resolve_components(&agent, inst_dir)?;
    let mut profile = Profile::default();
    for (comp, meta) in &components {
        profile.apply(comp, meta);
    }
    if profile.main_class.is_empty() && profile.traits.contains("legacyLaunch") {
        if let Ok(mojang) = load_mojang_version(&agent, &profile.mc_version) {
            profile.main_class = mojang
                .get("mainClass")
                .and_then(|v| v.as_str())
                .unwrap_or("net.minecraft.launchwrapper.Launch")
                .to_string();
            for lib in mojang.get("libraries").and_then(|l| l.as_array()).into_iter().flatten() {
                let name = lib.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let key = library_key(name);
                if lib.get("natives").is_none()
                    && !profile.libraries.iter().any(|l| library_key(l.get("name").and_then(|n| n.as_str()).unwrap_or("")) == key)
                {
                    profile.libraries.push(lib.clone());
                }
            }
        }
    }

    if let Some(service) = crate::device_key::skins_service_url() {
        let loader = components.iter().find_map(|(c, _)| match c.uid.as_str() {
            "net.fabricmc.fabric-loader" => Some("fabric"),
            "org.quiltmc.quilt-loader" => Some("quilt"),
            "net.minecraftforge" => Some("forge"),
            "net.neoforged" => Some("neoforge"),
            _ => None,
        });
        if let Some(loader) = loader {
            share_skins_in_instance(&agent, inst_dir, &gdir, &cfg, &profile.mc_version, loader, &service, &progress);
        }
    }

    let mut downloads: Vec<Download> = Vec::new();
    let mut native_jars: Vec<(PathBuf, Vec<String>)> = Vec::new();
    for lib in profile.libraries.iter().filter(|l| rules_allow(l)) {
        let has_artifact = lib.pointer("/downloads/artifact").is_some() || lib.get("natives").is_none();
        if has_artifact {
            if let Some(d) = library_download(lib, &libs_dir, None) {
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

    if let Some(log) = &profile.logging {
        if let (Some(id), Some(url)) = (
            log.pointer("/file/id").and_then(|v| v.as_str()),
            log.pointer("/file/url").and_then(|v| v.as_str()),
        ) {
            let path = assets_dir.join("log_configs").join(id);
            downloads.push(Download {
                url: url.to_string(),
                path,
                sha1: log.pointer("/file/sha1").and_then(|v| v.as_str()).map(str::to_string),
                size: log.pointer("/file/size").and_then(|v| v.as_u64()),
            });
        }
    }
    download_all(downloads, &progress, "libraries", "Descargando motor de Minecraft y librerías")?;

    if !profile.jar_mods.is_empty() {
        let _ = build_modded_jar(inst_dir, &main_jar.path, &profile.jar_mods);
    }

    if let Some(asset_index) = profile.asset_index.clone() {
        let asset_id = asset_index.get("id").and_then(|v| v.as_str()).unwrap_or("legacy").to_string();
        let index_path = assets_dir.join("indexes").join(format!("{}.json", asset_id));
        let index_stamp_path = assets_dir.join("indexes").join(format!("{}.ika-ok", asset_id));
        let expected_index_sha1 = asset_index.get("sha1").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        let index_dl = Download {
            url: asset_index.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            path: index_path.clone(),
            sha1: asset_index.get("sha1").and_then(|v| v.as_str()).map(str::to_string),
            size: asset_index.get("size").and_then(|v| v.as_u64()),
        };
        let is_legacy_layout = asset_id == "legacy" || asset_id == "pre-1.6";
        let fast_verified = !is_legacy_layout
            && index_dl.satisfied()
            && std::fs::read_to_string(&index_stamp_path)
                .map(|s| s.trim() == expected_index_sha1 && !expected_index_sha1.is_empty())
                .unwrap_or(false);
        if !fast_verified {
            let index_ok = index_dl.satisfied()
                && index_dl.sha1.as_ref().map_or(true, |s| sha1_file(&index_path).map_or(false, |h| h.eq_ignore_ascii_case(s)));
            if !index_ok {
                let _ = index_dl.fetch(&agent);
            }
            if let Ok(index_str) = std::fs::read_to_string(&index_path) {
                if let Ok(index) = serde_json::from_str::<Value>(&index_str) {
                    let objects = index.get("objects").and_then(|o| o.as_object()).cloned().unwrap_or_default();
                    let mut asset_dls = Vec::with_capacity(objects.len());
                    for obj in objects.values() {
                        let Some(hash) = obj.get("hash").and_then(|h| h.as_str()) else { continue };
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
                    let dl_ok = download_all(asset_dls, &progress, "assets", "Descargando recursos de Minecraft").is_ok();

                    let is_virtual = index.get("virtual").and_then(|v| v.as_bool()).unwrap_or(false);
                    let to_resources = index.get("map_to_resources").and_then(|v| v.as_bool()).unwrap_or(false);
                    if is_virtual || to_resources {
                        let game_assets = if to_resources {
                            gdir.join("resources")
                        } else {
                            assets_dir.join("virtual").join(&asset_id)
                        };
                        for (name, obj) in &objects {
                            if let Some(hash) = obj.get("hash").and_then(|h| h.as_str()) {
                                let src = assets_dir.join("objects").join(&hash[..2]).join(hash);
                                copy_if_missing(&src, &game_assets.join(name.replace('/', "\\")));
                            }
                        }
                    } else if dl_ok && !expected_index_sha1.is_empty() {
                        let _ = std::fs::write(&index_stamp_path, &expected_index_sha1);
                    }
                }
            }
        }
    }

    let _ = pick_java(inst_dir, &cfg, &profile, &progress);
    let natives_dir = inst_dir.join("natives");
    let _ = extract_natives(&native_jars, &natives_dir);
    let _ = ensure_authlib_injector(&agent);
    Ok(())
}

/// Downloads everything the instance needs and starts the game process.
pub(crate) fn prepare_and_spawn(
    progress: &Progress,
    inst_dir: &Path,
    username: &str,
    skin: Option<SkinInput>,
) -> Result<Started, String> {
    if !inst_dir.join("mmc-pack.json").exists() {
        return Err("No se encontró la instancia (falta mmc-pack.json).".to_string());
    }
    let inst_dir = inst_dir.to_path_buf();
    let username = username.to_string();
    let inst_name = inst_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let agent = agent();

    let root = ika_root();
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
                            if (n.ends_with(".jar") || n.ends_with(".jar.disabled")) && !n.starts_with('.') {
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
        for lib in mojang.get("libraries").and_then(|l| l.as_array()).into_iter().flatten() {
            let name = lib.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let key = library_key(name);
            if lib.get("natives").is_none()
                && !profile.libraries.iter().any(|l| library_key(l.get("name").and_then(|n| n.as_str()).unwrap_or("")) == key)
            {
                profile.libraries.push(lib.clone());
            }
        }
    }
    if profile.main_class.is_empty() {
        return Err("Esta versión de Minecraft no se puede iniciar (sin clase principal).".to_string());
    }

    // Opened-to-LAN worlds: SkinRestorer + the Ika skins config in modded instances, so
    // friends who join see everyone's published skin.
    if let Some(service) = crate::device_key::skins_service_url() {
        let loader = components.iter().find_map(|(c, _)| match c.uid.as_str() {
            "net.fabricmc.fabric-loader" => Some("fabric"),
            "org.quiltmc.quilt-loader" => Some("quilt"),
            "net.minecraftforge" => Some("forge"),
            "net.neoforged" => Some("neoforge"),
            _ => None,
        });
        if let Some(loader) = loader {
            share_skins_in_instance(&agent, &inst_dir, &gdir, &cfg, &profile.mc_version, loader, &service, progress);
        }
    }

    // 2. Library + main jar downloads
    let mut downloads: Vec<Download> = Vec::new();
    let mut classpath: Vec<PathBuf> = Vec::new();
    let mut native_jars: Vec<(PathBuf, Vec<String>)> = Vec::new();
    for lib in profile.libraries.iter().filter(|l| rules_allow(l)) {
        let has_artifact = lib.pointer("/downloads/artifact").is_some() || lib.get("natives").is_none();
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
                sha1: log.pointer("/file/sha1").and_then(|v| v.as_str()).map(str::to_string),
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
    download_all(downloads, progress, "libraries", "Descargando Minecraft y librerías")?;

    if !profile.jar_mods.is_empty() {
        progress.emit("jarmods", "Aplicando jar mods...", 0, 0);
        classpath[main_jar_slot] = build_modded_jar(&inst_dir, &main_jar.path, &profile.jar_mods)?;
    }

    // 3. Assets (skip parsing 450 KB JSON + 4,000 stat calls if already verified via .ika-ok stamp)
    let asset_index = profile.asset_index.clone().ok_or("Faltan los assets en los metadatos")?;
    let asset_id = asset_index.get("id").and_then(|v| v.as_str()).unwrap_or("legacy").to_string();
    let index_path = assets_dir.join("indexes").join(format!("{}.json", asset_id));
    let index_stamp_path = assets_dir.join("indexes").join(format!("{}.ika-ok", asset_id));
    let expected_index_sha1 = asset_index.get("sha1").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
    let index_dl = Download {
        url: asset_index.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        path: index_path.clone(),
        sha1: asset_index.get("sha1").and_then(|v| v.as_str()).map(str::to_string),
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
            && index_dl.sha1.as_ref().map_or(true, |s| sha1_file(&index_path).map_or(false, |h| h.eq_ignore_ascii_case(s)));
        if !index_ok {
            progress.emit("assets", "Descargando índice de assets...", 0, 0);
            index_dl.fetch(&agent)?;
        }
        let index: Value = serde_json::from_str(&std::fs::read_to_string(&index_path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Índice de assets inválido: {}", e))?;
        let objects = index.get("objects").and_then(|o| o.as_object()).cloned().unwrap_or_default();
        let mut asset_dls = Vec::with_capacity(objects.len());
        for obj in objects.values() {
            let Some(hash) = obj.get("hash").and_then(|h| h.as_str()) else { continue };
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
        let is_virtual = index.get("virtual").and_then(|v| v.as_bool()).unwrap_or(false);
        let to_resources = index.get("map_to_resources").and_then(|v| v.as_bool()).unwrap_or(false);
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
    let java = pick_java(&inst_dir, &cfg, &profile, progress)?;
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
        let version = Some(field("SEMANTIC_VERSION")).filter(|v| !v.is_empty()).unwrap_or_else(|| field("JAVA_VERSION"));
        let vendor = if vendor.is_empty() { "Java".to_string() } else { vendor };
        progress.emit("java-info", &format!("Java: {} {} — {}", vendor, version, java.display()), 0, 0);
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

    // Profile skin: local skin server + authlib-injector. A problem here never stops the
    // launch, the game just shows the default skin.
    let mut skin_agent: Option<String> = None;
    if let Some(skin) = skin {
        progress.emit("skin", "Aplicando tu skin...", 0, 0);
        // Publish it to the Ika skins service in a background thread so network latency never delays game launch.
        if let Some(service) = crate::device_key::skins_service_url() {
            let username_bg = username.clone();
            let png_bg = skin.png.clone();
            let progress_bg = progress.clone();
            let agent_bg = agent.clone();
            std::thread::spawn(move || {
                match crate::device_key::publish_skin(&agent_bg, &service, &username_bg, &png_bg) {
                    Ok(crate::device_key::PublishOutcome::Published { claimed }) => progress_bg.emit(
                        "skin-published",
                        &if claimed {
                            format!("El nombre {} ahora es tuyo: otros jugadores verán tu skin en servidores con SkinRestorer", username_bg)
                        } else {
                            "Tu skin está publicada: otros jugadores la verán en servidores con SkinRestorer".to_string()
                        },
                        0,
                        0,
                    ),
                    Ok(crate::device_key::PublishOutcome::Unchanged) => {}
                    Ok(crate::device_key::PublishOutcome::NameTaken) => progress_bg.emit(
                        "skin-warn",
                        &format!("El nombre {} pertenece a otro PC: en servidores con SkinRestorer se verá su skin, no la tuya", username_bg),
                        0,
                        0,
                    ),
                    Err(e) => progress_bg.emit("skin-warn", &format!("No se pudo publicar tu skin ({})", e), 0, 0),
                }
            });
        }
        let setup = ensure_authlib_injector(&agent).and_then(|jar| {
            crate::skin_server::register(&username, &uuid, skin.png, skin.slim)
                .map(|url| format!("-javaagent:{}={}", jar.to_string_lossy(), url))
        });
        match setup {
            Ok(arg) => skin_agent = Some(arg),
            Err(e) => progress.emit("skin-warn", &format!("No se pudo aplicar tu skin ({}); se usará la predeterminada", e), 0, 0),
        }
    }
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
    tokens.insert("user_type", if skin_agent.is_some() { "mojang" } else { "legacy" }.to_string());
    tokens.insert("user_properties", "{}".to_string());
    tokens.insert("version_name", profile.mc_version.clone());
    tokens.insert("version_type", profile.version_type.clone());
    tokens.insert("profile_name", inst_name.clone());
    tokens.insert("game_directory", gdir.to_string_lossy().to_string());
    tokens.insert("assets_root", assets_dir.to_string_lossy().to_string());
    tokens.insert("assets_index_name", asset_id.clone());
    tokens.insert("game_assets", game_assets.to_string_lossy().to_string());
    tokens.insert("library_directory", libs_dir.to_string_lossy().to_string());
    tokens.insert("natives_directory", natives_dir.to_string_lossy().to_string());
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
    let has_custom_gc = user_jvm_args.iter().any(|a| a.contains("GC") || a.starts_with("-XX:+Use"));
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
    args.push("-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump".to_string());
    let mem = |k: &str, d: u32| cfg.get(k).and_then(|v| v.parse::<u32>().ok()).unwrap_or(d);
    let (min_mem, max_mem) = if cfg.get("OverrideMemory").map_or(false, |v| v == "true") {
        (mem("MinMemAlloc", 512), mem("MaxMemAlloc", 4096))
    } else {
        (512, 4096)
    };
    args.push(format!("-Xms{}m", min_mem.min(max_mem)));
    args.push(format!("-Xmx{}m", min_mem.max(max_mem)));
    if !profile.jar_mods.is_empty() {
        // Same as Prism: old FML refuses a minecraft.jar that jar mods have modified.
        args.push("-Dfml.ignoreInvalidMinecraftCertificates=true".to_string());
        args.push("-Dfml.ignorePatchDiscrepancies=true".to_string());
    }
    args.extend(profile.jvm_args.iter().map(|a| replace_tokens(a, &tokens)));
    if let Some(agent_arg) = &skin_agent {
        args.push(agent_arg.clone());
        // Don't print authlib-injector's server name on the title screen.
        args.push("-Dauthlibinjector.noShowServerName".to_string());
    }
    args.push(format!("-Djava.library.path={}", natives_dir.to_string_lossy()));
    args.push("-Dminecraft.launcher.brand=IkaLauncher".to_string());
    args.push(format!("-Dminecraft.launcher.version={}", env!("CARGO_PKG_VERSION")));
    if let Some(a) = logging_arg {
        args.push(a);
    }
    if profile.main_class == FORGEWRAPPER_MAIN {
        // ForgeWrapper runs the Forge/NeoForge installer processors on first launch.
        let installer = profile
            .maven_files
            .iter()
            .find(|f| f.get("name").and_then(|n| n.as_str()).map_or(false, |n| n.ends_with(":installer")))
            .and_then(|f| library_download(f, &libs_dir, None))
            .ok_or("No se encontró el instalador de Forge en los metadatos")?;
        args.push(format!("-Dforgewrapper.librariesDir={}", libs_dir.to_string_lossy()));
        args.push(format!("-Dforgewrapper.installer={}", installer.path.to_string_lossy()));
        args.push(format!("-Dforgewrapper.minecraft={}", main_jar.path.to_string_lossy()));
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
        if let Some(addr) = cfg.get("JoinServerOnLaunchAddress").map(|a| a.trim()).filter(|a| !a.is_empty()) {
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
    let out_log = std::fs::File::create(logs_dir.join("ika-launcher-output.log")).map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(&java_exe);
    cmd.args(&args)
        .current_dir(&gdir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    progress.emit("launch", "Iniciando Minecraft...", 0, 0);
    let started_at = std::time::SystemTime::now();
    let mut child = cmd.spawn().map_err(|e| format!("No se pudo iniciar Java ({}): {}", java_exe.display(), e))?;
    let log = std::sync::Arc::new(Mutex::new(out_log));
    let mut output_threads = Vec::new();
    if let Some(out) = child.stdout.take() {
        output_threads.push(spawn_output_reader(out, log.clone(), progress.app.clone(), progress.instance.clone()));
    }
    if let Some(err) = child.stderr.take() {
        output_threads.push(spawn_output_reader(err, log, progress.app.clone(), progress.instance.clone()));
    }
    Ok(Started {
        child,
        output_log: logs_dir.join("ika-launcher-output.log"),
        mc_version: profile.mc_version.clone(),
        game_dir: gdir.clone(),
        started_at,
        output_threads,
    })
}

// ---------------------------------------------------------------------------------------
// Loader version resolution & Modpack installation (Modrinth .mrpack / CurseForge .zip)
// ---------------------------------------------------------------------------------------

const CF_API_KEY: &str = "$2a$10$bL4bIL5pUWqfcO7KQtnMReakwtfHbNKh6v1uTpKlzhwoueEJQnPnm";

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
        if !p.extension().map_or(false, |e| e.eq_ignore_ascii_case("jar")) {
            continue;
        }
        let Ok(file) = std::fs::File::open(&p) else { continue };
        let Ok(mut archive) = zip::ZipArchive::new(file) else { continue };
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
    for prefix in ["neoforge-", "forge-", "fabric-loader-", "fabric-", "quilt-loader-", "quilt-"] {
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
                                        req.get("uid").and_then(|u| u.as_str()) == Some("net.minecraft")
                                            && req.get("equals").and_then(|e| e.as_str()) == Some(mc_version)
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
                        || versions
                            .iter()
                            .any(|v| v.get("version").and_then(|s| s.as_str()) == Some(pref.as_str()))
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

/// Resolves a valid Prism loader version for `(loader, mc_version)`.
pub fn resolve_loader_version(mc_version: &str, loader: &str, preferred: Option<&str>) -> Option<String> {
    let (uid, _, _) = loader_uid_and_name(loader)?;
    let ag = agent();
    resolve_loader_uid_version(&ag, uid, mc_version, preferred)
}

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ModpackInstallSpec {
    pub platform: String,
    pub project_id: Option<String>,
    pub cf_id: Option<u64>,
    pub slug: Option<String>,
    pub version_id: Option<String>,
    pub title: Option<String>,
}

pub struct ModpackInstallOutcome {
    pub mc_version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub pack_type: String,
    pub pack_id: String,
    pub pack_name: String,
}

fn emit_modpack_progress(app: &Option<tauri::AppHandle>, inst_name: &str, stage: &str, message: &str, done: usize, total: usize) {
    emit_modpack_progress_with_speed(app, inst_name, stage, message, done, total, None);
}

fn emit_modpack_progress_with_speed(app: &Option<tauri::AppHandle>, inst_name: &str, stage: &str, message: &str, done: usize, total: usize, speed: Option<&str>) {
    let Some(app) = app else { return };
    let mut payload = serde_json::json!({
        "instance": inst_name,
        "stage": stage,
        "message": message,
        "done": done,
        "total": total,
    });
    if let Some(s) = speed {
        payload["speed"] = serde_json::Value::String(s.to_string());
    }
    let _ = app.emit("modpack-install-progress", payload.clone());
    let _ = app.emit("mc-launch-progress", payload);
}

fn sanitize_relative_path(raw: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in raw.split(['/', '\\']) {
        let trimmed = part.trim();
        if trimmed.is_empty() || trimmed == "." {
            continue;
        }
        if trimmed == ".." {
            return None;
        }
        out.push(trimmed);
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

fn extract_zip_overrides(archive: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, prefixes: &[&str], mc_dir: &Path) -> Result<(), String> {
    for idx in 0..archive.len() {
        let mut entry = archive.by_index(idx).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        let rel = prefixes.iter().find_map(|prefix| {
            let p = prefix.trim_end_matches('/');
            let with_slash = format!("{}/", p);
            name.strip_prefix(&with_slash)
        });
        let Some(rel_str) = rel else { continue };
        let Some(clean_rel) = sanitize_relative_path(rel_str) else { continue };
        let dest = mc_dir.join(clean_rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out_file = std::fs::File::create(&dest).map_err(|e| format!("{}: {}", dest.display(), e))?;
        std::io::copy(&mut entry, &mut out_file).map_err(|e| format!("{}: {}", dest.display(), e))?;
    }
    Ok(())
}

fn url_encode_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn cf_cdn_url(host: &str, file_id: u64, file_name: &str) -> String {
    let major = file_id / 1000;
    let minor = file_id % 1000;
    format!("https://{}/files/{}/{}/{}", host, major, minor, url_encode_simple(file_name))
}

fn cf_get_json(agent: &ureq::Agent, url: &str) -> Result<Value, String> {
    let resp = agent
        .get(url)
        .set("x-api-key", CF_API_KEY)
        .set("Accept", "application/json")
        .call()
        .map_err(|e| format!("{}: {}", url, e))?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf).map_err(|e| e.to_string())?;
    serde_json::from_slice(&buf).map_err(|e| e.to_string())
}

fn cf_post_json(agent: &ureq::Agent, url: &str, body: &Value) -> Result<Value, String> {
    let resp = agent
        .post(url)
        .set("x-api-key", CF_API_KEY)
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .send_string(&body.to_string())
        .map_err(|e| format!("{}: {}", url, e))?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut buf).map_err(|e| e.to_string())?;
    serde_json::from_slice(&buf).map_err(|e| e.to_string())
}

#[derive(Clone)]
struct ModpackFileTask {
    urls: Vec<String>,
    path: PathBuf,
    sha1: Option<String>,
}

fn fetch_modpack_task(agent: &ureq::Agent, task: &ModpackFileTask) -> Result<usize, String> {
    if let Ok(m) = std::fs::metadata(&task.path) {
        if m.is_file() && m.len() > 0 {
            return Ok(0);
        }
    }
    let mut candidate_urls = task.urls.clone();
    // If all direct URLs fail and we have a SHA-1, Modrinth's hash lookup often hosts the exact same jar.
    let mut tried_modrinth_fallback = false;
    let mut last_err = String::new();

    let mut idx = 0;
    while idx < candidate_urls.len() {
        let url = candidate_urls[idx].clone();
        idx += 1;
        if url.is_empty() {
            continue;
        }
        for attempt in 0..2 {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(400));
            }
            match http_get_bytes(agent, &url) {
                Ok(data) if !data.is_empty() => {
                    if let Some(expected) = &task.sha1 {
                        let got = sha1_hex(&data);
                        if !got.eq_ignore_ascii_case(expected) {
                            last_err = format!("Checksum distinto en {}", url);
                            continue;
                        }
                    }
                    if let Some(parent) = task.path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                    }
                    let mut tmp = task.path.clone().into_os_string();
                    tmp.push(".part");
                    let tmp = PathBuf::from(tmp);
                    let len = data.len();
                    std::fs::write(&tmp, &data).map_err(|e| e.to_string())?;
                    std::fs::rename(&tmp, &task.path).map_err(|e| e.to_string())?;
                    return Ok(len);
                }
                Ok(_) => last_err = format!("Respuesta vacía de {}", url),
                Err(e) => last_err = e,
            }
        }

        if idx == candidate_urls.len() && !tried_modrinth_fallback {
            tried_modrinth_fallback = true;
            if let Some(sha1) = &task.sha1 {
                let mr_url = format!("https://api.modrinth.com/v2/version_file/{}?algorithm=sha1", sha1);
                if let Ok(bytes) = http_get_bytes(agent, &mr_url) {
                    if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                        if let Some(files) = v.get("files").and_then(|f| f.as_array()) {
                            for f in files {
                                if let Some(u) = f.get("url").and_then(|u| u.as_str()) {
                                    candidate_urls.push(u.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Err(last_err)
}

fn download_modpack_tasks(
    tasks: Vec<ModpackFileTask>,
    app: &Option<tauri::AppHandle>,
    inst_name: &str,
) -> Result<(), String> {
    use rayon::prelude::*;
    let mut seen = HashSet::new();
    let pending: Vec<ModpackFileTask> = tasks
        .into_iter()
        .filter(|t| seen.insert(t.path.clone()))
        .collect();
    let total = pending.len();
    if total == 0 {
        return Ok(());
    }
    emit_modpack_progress(
        app,
        inst_name,
        "modpack-mods",
        &format!("Descargando archivos del modpack (0/{})...", total),
        0,
        total,
    );
    let done = AtomicUsize::new(0);
    let downloaded_bytes = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let start_time = std::time::Instant::now();
    let last_emit = Mutex::new(std::time::Instant::now());
    let first_err = Mutex::new(None::<String>);

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(DOWNLOAD_THREADS)
        .build()
        .map_err(|e| e.to_string())?;

    pool.install(|| {
        pending.par_iter().for_each_init(agent, |ag, task| {
            match fetch_modpack_task(ag, task) {
                Ok(bytes) => {
                    downloaded_bytes.fetch_add(bytes, Ordering::Relaxed);
                }
                Err(e) => {
                    failed.fetch_add(1, Ordering::SeqCst);
                    let mut fe = first_err.lock().unwrap_or_else(|e| e.into_inner());
                    if fe.is_none() {
                        *fe = Some(e);
                    }
                }
            }
            let n = done.fetch_add(1, Ordering::SeqCst) + 1;
            let mut last = last_emit.lock().unwrap_or_else(|e| e.into_inner());
            if n == total || last.elapsed().as_millis() > 100 {
                *last = std::time::Instant::now();
                let elapsed = start_time.elapsed().as_secs_f64();
                let bytes = downloaded_bytes.load(Ordering::Relaxed);
                let speed_str = if elapsed > 0.25 && bytes > 0 {
                    format_download_speed((bytes as f64) / elapsed)
                } else {
                    String::new()
                };
                emit_modpack_progress_with_speed(
                    app,
                    inst_name,
                    "modpack-mods",
                    &format!("Descargando mods del modpack ({}/{})...", n, total),
                    n,
                    total,
                    if speed_str.is_empty() { None } else { Some(&speed_str) },
                );
            }
        });
    });

    let fail_count = failed.load(Ordering::SeqCst);
    // Only fail the whole installation if nearly everything failed (e.g. offline).
    if fail_count == total {
        let msg = first_err
            .into_inner()
            .unwrap_or_default()
            .unwrap_or_else(|| "Sin conexión".to_string());
        return Err(format!("No se pudieron descargar los archivos del modpack: {}", msg));
    }
    Ok(())
}

/// Downloads and unpacks a Modrinth (`.mrpack`) or CurseForge (`.zip`) modpack into `mc_dir`.
pub fn install_modpack(
    app: Option<tauri::AppHandle>,
    inst_name: &str,
    mc_dir: &Path,
    spec: &ModpackInstallSpec,
    fallback_mc: &str,
    fallback_loader: &str,
) -> Result<ModpackInstallOutcome, String> {
    let ag = agent();
    let platform = spec.platform.trim().to_lowercase();
    let pack_title = spec
        .title
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(inst_name)
        .to_string();

    if platform == "curseforge" {
        emit_modpack_progress(&app, inst_name, "modpack-meta", "Consultando versión del modpack en CurseForge...", 0, 1);

        // 1. Resolve CurseForge numeric modId
        let mut mod_id = spec.cf_id.filter(|&id| id > 0).or_else(|| {
            spec.project_id
                .as_deref()
                .and_then(|s| s.trim_start_matches("cf:").parse::<u64>().ok())
                .filter(|&id| id > 0)
        });

        if mod_id.is_none() {
            if let Some(slug) = spec.slug.as_deref().filter(|s| !s.is_empty()) {
                let url = format!(
                    "https://api.curseforge.com/v1/mods/search?gameId=432&classId=4471&slug={}",
                    url_encode_simple(slug)
                );
                if let Ok(res) = cf_get_json(&ag, &url) {
                    mod_id = res
                        .pointer("/data/0/id")
                        .and_then(|v| v.as_u64());
                }
            }
        }
        if mod_id.is_none() {
            let query = spec
                .title
                .as_deref()
                .or(spec.slug.as_deref())
                .unwrap_or(inst_name);
            let url = format!(
                "https://api.curseforge.com/v1/mods/search?gameId=432&classId=4471&searchFilter={}&sortField=2&sortOrder=desc&pageSize=10",
                url_encode_simple(query)
            );
            if let Ok(res) = cf_get_json(&ag, &url) {
                if let Some(arr) = res.get("data").and_then(|d| d.as_array()) {
                    let slug_low = spec.slug.as_deref().unwrap_or("").to_lowercase();
                    let title_low = query.to_lowercase();
                    mod_id = arr
                        .iter()
                        .find(|item| {
                            item.get("slug")
                                .and_then(|s| s.as_str())
                                .map_or(false, |s| s.eq_ignore_ascii_case(&slug_low))
                                || item
                                    .get("name")
                                    .and_then(|n| n.as_str())
                                    .map_or(false, |n| n.to_lowercase() == title_low)
                        })
                        .or_else(|| arr.first())
                        .and_then(|item| item.get("id").and_then(|id| id.as_u64()));
                }
            }
        }

        let mod_id = mod_id.ok_or_else(|| "No se encontró el modpack en CurseForge.".to_string())?;

        // 2. Resolve CurseForge modpack file
        let mut file_obj: Option<Value> = None;
        if let Some(vid) = spec
            .version_id
            .as_deref()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|&id| id > 0)
        {
            let url = format!("https://api.curseforge.com/v1/mods/{}/files/{}", mod_id, vid);
            if let Ok(res) = cf_get_json(&ag, &url) {
                file_obj = res.get("data").cloned();
            }
        }

        if file_obj.is_none() {
            let urls = [
                format!(
                    "https://api.curseforge.com/v1/mods/{}/files?pageSize=25&gameVersion={}",
                    mod_id,
                    url_encode_simple(fallback_mc)
                ),
                format!("https://api.curseforge.com/v1/mods/{}/files?pageSize=25", mod_id),
            ];
            for u in &urls {
                if let Ok(res) = cf_get_json(&ag, u) {
                    if let Some(arr) = res.get("data").and_then(|d| d.as_array()) {
                        let pick = arr
                            .iter()
                            .find(|f| {
                                f.get("fileName")
                                    .and_then(|n| n.as_str())
                                    .map_or(false, |n| n.to_lowercase().ends_with(".zip"))
                                    && f.get("releaseType").and_then(|r| r.as_u64()) == Some(1)
                            })
                            .or_else(|| {
                                arr.iter().find(|f| {
                                    f.get("fileName")
                                        .and_then(|n| n.as_str())
                                        .map_or(false, |n| n.to_lowercase().ends_with(".zip"))
                                })
                            })
                            .or_else(|| arr.first());
                        if let Some(p) = pick {
                            file_obj = Some(p.clone());
                            break;
                        }
                    }
                }
            }
        }

        let file_obj = file_obj.ok_or_else(|| "No se encontró un archivo descargable para este modpack en CurseForge.".to_string())?;
        let pack_file_id = file_obj.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
        let pack_file_name = file_obj
            .get("fileName")
            .and_then(|v| v.as_str())
            .unwrap_or("modpack.zip");

        let mut zip_urls = Vec::new();
        if let Some(dl) = file_obj.get("downloadUrl").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            zip_urls.push(dl.to_string());
        }
        if pack_file_id > 0 {
            zip_urls.push(cf_cdn_url("edge.forgecdn.net", pack_file_id, pack_file_name));
            zip_urls.push(cf_cdn_url("mediafilez.forgecdn.net", pack_file_id, pack_file_name));
        }

        emit_modpack_progress(&app, inst_name, "modpack-zip", "Descargando paquete del modpack...", 0, 1);
        let mut zip_bytes = None;
        let mut last_err = String::new();
        for u in &zip_urls {
            match http_get_bytes(&ag, u) {
                Ok(b) if !b.is_empty() => {
                    zip_bytes = Some(b);
                    break;
                }
                Ok(_) => last_err = "Archivo de modpack vacío".to_string(),
                Err(e) => last_err = e,
            }
        }
        let zip_bytes = zip_bytes.ok_or_else(|| format!("No se pudo descargar el paquete del modpack: {}", last_err))?;

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
            .map_err(|e| format!("El paquete del modpack no es un archivo ZIP válido: {}", e))?;

        let manifest: Value = {
            let mut mf = archive
                .by_name("manifest.json")
                .map_err(|_| "El modpack de CurseForge no contiene manifest.json".to_string())?;
            let mut s = String::new();
            std::io::Read::read_to_string(&mut mf, &mut s).map_err(|e| e.to_string())?;
            serde_json::from_str(&s).map_err(|e| format!("manifest.json inválido: {}", e))?
        };

        let resolved_mc = manifest
            .pointer("/minecraft/version")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(fallback_mc)
            .to_string();

        let mut resolved_loader = fallback_loader.to_string();
        let mut resolved_loader_ver: Option<String> = None;
        if let Some(loaders) = manifest.pointer("/minecraft/modLoaders").and_then(|l| l.as_array()) {
            let chosen = loaders
                .iter()
                .find(|l| l.get("primary").and_then(|p| p.as_bool()) == Some(true))
                .or_else(|| loaders.first())
                .and_then(|l| l.get("id").and_then(|id| id.as_str()));
            if let Some(id_str) = chosen {
                let low = id_str.to_lowercase();
                if let Some(rest) = low.strip_prefix("neoforge-") {
                    resolved_loader = "NeoForge".to_string();
                    resolved_loader_ver = Some(clean_loader_version_str(rest, &resolved_mc));
                } else if let Some(rest) = low.strip_prefix("forge-") {
                    resolved_loader = "Forge".to_string();
                    resolved_loader_ver = Some(clean_loader_version_str(rest, &resolved_mc));
                } else if let Some(rest) = low.strip_prefix("fabric-") {
                    resolved_loader = "Fabric".to_string();
                    resolved_loader_ver = Some(clean_loader_version_str(rest, &resolved_mc));
                } else if let Some(rest) = low.strip_prefix("quilt-") {
                    resolved_loader = "Quilt".to_string();
                    resolved_loader_ver = Some(clean_loader_version_str(rest, &resolved_mc));
                }
            }
        }

        let overrides_dir = manifest
            .get("overrides")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("overrides")
            .to_string();

        emit_modpack_progress(&app, inst_name, "modpack-overrides", "Extrayendo configuración y overrides...", 0, 1);
        extract_zip_overrides(&mut archive, &[&overrides_dir, "client-overrides"], mc_dir)?;

        // 3. Batch resolve all fileIDs in manifest.json
        let file_entries: Vec<(u64, u64)> = manifest
            .get("files")
            .and_then(|f| f.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|item| item.get("required").and_then(|r| r.as_bool()).unwrap_or(true))
                    .filter_map(|item| {
                        let pid = item.get("projectID").and_then(|v| v.as_u64())?;
                        let fid = item.get("fileID").and_then(|v| v.as_u64())?;
                        Some((pid, fid))
                    })
                    .collect()
            })
            .unwrap_or_default();

        if !file_entries.is_empty() {
            emit_modpack_progress(
                &app,
                inst_name,
                "modpack-resolve",
                &format!("Resolviendo {} mods de CurseForge...", file_entries.len()),
                0,
                file_entries.len(),
            );

            let mut tasks: Vec<ModpackFileTask> = Vec::with_capacity(file_entries.len());
            let all_fids: Vec<u64> = file_entries.iter().map(|(_, fid)| *fid).collect();

            for chunk in all_fids.chunks(300) {
                let body = serde_json::json!({ "fileIds": chunk });
                if let Ok(res) = cf_post_json(&ag, "https://api.curseforge.com/v1/mods/files", &body) {
                    if let Some(data) = res.get("data").and_then(|d| d.as_array()) {
                        for f in data {
                            let Some(fid) = f.get("id").and_then(|v| v.as_u64()) else { continue };
                            let Some(fname) = f.get("fileName").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) else {
                                continue;
                            };
                            let Some(clean_fname) = sanitize_relative_path(fname) else { continue };

                            let fname_low = fname.to_lowercase();
                            let subfolder = if fname_low.ends_with(".jar") {
                                "mods"
                            } else if fname_low.ends_with(".zip") {
                                let has_shaders = f
                                    .get("modules")
                                    .and_then(|m| m.as_array())
                                    .map_or(false, |mods| {
                                        mods.iter().any(|m| {
                                            m.get("name")
                                                .and_then(|n| n.as_str())
                                                .map_or(false, |n| n.eq_ignore_ascii_case("shaders") || n.eq_ignore_ascii_case("shader"))
                                        })
                                    });
                                if has_shaders {
                                    "shaderpacks"
                                } else {
                                    "resourcepacks"
                                }
                            } else {
                                "mods"
                            };

                            let sha1 = f
                                .get("hashes")
                                .and_then(|h| h.as_array())
                                .and_then(|arr| {
                                    arr.iter()
                                        .find(|h| h.get("algo").and_then(|a| a.as_u64()) == Some(1))
                                        .and_then(|h| h.get("value").and_then(|v| v.as_str()))
                                })
                                .map(str::to_string);

                            let mut urls = Vec::new();
                            if let Some(dl) = f.get("downloadUrl").and_then(|u| u.as_str()).filter(|s| !s.is_empty()) {
                                urls.push(dl.to_string());
                            }
                            urls.push(cf_cdn_url("edge.forgecdn.net", fid, fname));
                            urls.push(cf_cdn_url("mediafilez.forgecdn.net", fid, fname));

                            tasks.push(ModpackFileTask {
                                urls,
                                path: mc_dir.join(subfolder).join(clean_fname),
                                sha1,
                            });
                        }
                    }
                }
            }

            download_modpack_tasks(tasks, &app, inst_name)?;
        }

        let loader_version = resolve_loader_uid_version(
            &ag,
            loader_uid_and_name(&resolved_loader)
                .map(|(u, _, _)| u)
                .unwrap_or("net.minecraftforge"),
            &resolved_mc,
            resolved_loader_ver.as_deref(),
        );

        return Ok(ModpackInstallOutcome {
            mc_version: resolved_mc,
            loader: resolved_loader,
            loader_version,
            pack_type: "curseforge".to_string(),
            pack_id: mod_id.to_string(),
            pack_name: pack_title,
        });
    }

    // -------------------------------------------------------------------------
    // Modrinth (.mrpack)
    // -------------------------------------------------------------------------
    emit_modpack_progress(&app, inst_name, "modpack-meta", "Consultando versión del modpack en Modrinth...", 0, 1);

    let mut version_obj: Option<Value> = None;

    // 1. Try explicit version ID if it's a real Modrinth ID
    if let Some(vid) = spec
        .version_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "latest")
    {
        let url = format!("https://api.modrinth.com/v2/version/{}", url_encode_simple(vid));
        if let Ok(bytes) = http_get_bytes(&ag, &url) {
            if let Ok(v) = serde_json::from_slice::<Value>(&bytes) {
                if v.get("files").is_some() {
                    version_obj = Some(v);
                }
            }
        }
    }

    // 2. Otherwise fetch project versions by slug or project_id (or search by title)
    if version_obj.is_none() {
        let mut candidates = Vec::new();
        if let Some(s) = spec.slug.as_deref().filter(|s| !s.is_empty()) {
            candidates.push(s.to_string());
        }
        if let Some(p) = spec.project_id.as_deref().filter(|p| !p.is_empty()) {
            if !candidates.contains(&p.to_string()) {
                candidates.push(p.to_string());
            }
        }

        let mut versions_list: Option<Vec<Value>> = None;
        for cand in &candidates {
            let url = format!("https://api.modrinth.com/v2/project/{}/version", url_encode_simple(cand));
            if let Ok(bytes) = http_get_bytes(&ag, &url) {
                if let Ok(arr) = serde_json::from_slice::<Vec<Value>>(&bytes) {
                    if !arr.is_empty() {
                        versions_list = Some(arr);
                        break;
                    }
                }
            }
        }

        if versions_list.is_none() {
            let query = spec.title.as_deref().or(spec.slug.as_deref()).unwrap_or(inst_name);
            let search_url = format!(
                "https://api.modrinth.com/v2/search?query={}&facets=%5B%5B%22project_type:modpack%22%5D%5D&limit=5",
                url_encode_simple(query)
            );
            if let Ok(bytes) = http_get_bytes(&ag, &search_url) {
                if let Ok(res) = serde_json::from_slice::<Value>(&bytes) {
                    if let Some(pid) = res.pointer("/hits/0/project_id").and_then(|v| v.as_str()) {
                        let url = format!("https://api.modrinth.com/v2/project/{}/version", url_encode_simple(pid));
                        if let Ok(vbytes) = http_get_bytes(&ag, &url) {
                            if let Ok(arr) = serde_json::from_slice::<Vec<Value>>(&vbytes) {
                                if !arr.is_empty() {
                                    versions_list = Some(arr);
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(arr) = versions_list {
            let matches_mc = |v: &Value| {
                v.get("game_versions")
                    .and_then(|gv| gv.as_array())
                    .map_or(false, |gvs| gvs.iter().any(|g| g.as_str() == Some(fallback_mc)))
            };
            version_obj = arr
                .iter()
                .find(|v| matches_mc(v) && v.get("version_type").and_then(|t| t.as_str()) == Some("release"))
                .or_else(|| arr.iter().find(|v| matches_mc(v)))
                .or_else(|| arr.first())
                .cloned();
        }
    }

    let version_obj = version_obj.ok_or_else(|| "No se encontró una versión descargable para este modpack en Modrinth.".to_string())?;
    let pack_id = version_obj
        .get("project_id")
        .and_then(|v| v.as_str())
        .or(spec.project_id.as_deref())
        .or(spec.slug.as_deref())
        .unwrap_or("")
        .to_string();

    let files = version_obj
        .get("files")
        .and_then(|f| f.as_array())
        .ok_or_else(|| "La versión del modpack en Modrinth no contiene archivos.".to_string())?;
    let mrpack_file = files
        .iter()
        .find(|f| f.get("primary").and_then(|p| p.as_bool()) == Some(true))
        .or_else(|| {
            files.iter().find(|f| {
                f.get("filename")
                    .and_then(|n| n.as_str())
                    .map_or(false, |n| n.ends_with(".mrpack"))
            })
        })
        .or_else(|| files.first())
        .ok_or_else(|| "No se encontró el archivo .mrpack en Modrinth.".to_string())?;

    let mrpack_url = mrpack_file
        .get("url")
        .and_then(|u| u.as_str())
        .ok_or_else(|| "El archivo .mrpack no tiene URL de descarga.".to_string())?;

    emit_modpack_progress(&app, inst_name, "modpack-zip", "Descargando paquete .mrpack...", 0, 1);
    let mrpack_bytes = http_get_bytes(&ag, mrpack_url)
        .map_err(|e| format!("No se pudo descargar el archivo .mrpack: {}", e))?;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(mrpack_bytes))
        .map_err(|e| format!("El archivo .mrpack está dañado: {}", e))?;

    let index_json: Value = {
        let mut idx_file = archive
            .by_name("modrinth.index.json")
            .map_err(|_| "El archivo .mrpack no contiene modrinth.index.json".to_string())?;
        let mut s = String::new();
        std::io::Read::read_to_string(&mut idx_file, &mut s).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| format!("modrinth.index.json inválido: {}", e))?
    };

    let deps = index_json.get("dependencies").and_then(|d| d.as_object());
    let resolved_mc = deps
        .and_then(|d| d.get("minecraft"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback_mc)
        .to_string();

    let (resolved_loader, raw_loader_ver) = if let Some(d) = deps {
        if let Some(v) = d.get("fabric-loader").and_then(|v| v.as_str()) {
            ("Fabric".to_string(), Some(v.to_string()))
        } else if let Some(v) = d.get("quilt-loader").and_then(|v| v.as_str()) {
            ("Quilt".to_string(), Some(v.to_string()))
        } else if let Some(v) = d.get("neoforge").and_then(|v| v.as_str()) {
            ("NeoForge".to_string(), Some(v.to_string()))
        } else if let Some(v) = d.get("forge").and_then(|v| v.as_str()) {
            ("Forge".to_string(), Some(v.to_string()))
        } else {
            (fallback_loader.to_string(), None)
        }
    } else {
        (fallback_loader.to_string(), None)
    };

    emit_modpack_progress(&app, inst_name, "modpack-overrides", "Extrayendo configuración y overrides...", 0, 1);
    extract_zip_overrides(&mut archive, &["overrides", "client-overrides"], mc_dir)?;

    let mut tasks: Vec<ModpackFileTask> = Vec::new();
    if let Some(pack_files) = index_json.get("files").and_then(|f| f.as_array()) {
        for pf in pack_files {
            if pf.pointer("/env/client").and_then(|c| c.as_str()) == Some("unsupported") {
                continue;
            }
            let Some(rel_str) = pf.get("path").and_then(|p| p.as_str()) else { continue };
            let Some(clean_rel) = sanitize_relative_path(rel_str) else { continue };
            let urls: Vec<String> = pf
                .get("downloads")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|u| u.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if urls.is_empty() {
                continue;
            }
            let sha1 = pf
                .pointer("/hashes/sha1")
                .and_then(|h| h.as_str())
                .map(str::to_string);
            tasks.push(ModpackFileTask {
                urls,
                path: mc_dir.join(clean_rel),
                sha1,
            });
        }
    }

    download_modpack_tasks(tasks, &app, inst_name)?;

    let loader_version = resolve_loader_uid_version(
        &ag,
        loader_uid_and_name(&resolved_loader)
            .map(|(u, _, _)| u)
            .unwrap_or("net.fabricmc.fabric-loader"),
        &resolved_mc,
        raw_loader_ver.as_deref(),
    );

    Ok(ModpackInstallOutcome {
        mc_version: resolved_mc,
        loader: resolved_loader,
        loader_version,
        pack_type: "modrinth".to_string(),
        pack_id,
        pack_name: pack_title,
    })
}

