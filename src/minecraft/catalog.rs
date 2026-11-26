//! Catalog discovery from Ika WebView2. Reads external instances in place.
use super::engine as mc;

#[derive(Clone, Debug)]
pub struct LauncherInstance {
    pub name: String,
    pub path: String,
    pub mod_count: usize,
    pub mc_version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub icon_path: Option<String>,
    pub java_version: Option<String>,
    pub java_vendor: &'static str,
    pub total_time_played: Option<u64>,
    pub shader_count: usize,
    pub resourcepack_count: usize,
    /// `MaxMemAlloc` from instance.cfg, 8192 MB when absent (as WebView2).
    pub max_mem_mb: u32,
}

/// Reads custom `InstanceDir=...` from a Prism/Freesm/ElyPrism configuration file if set.
fn custom_instance_dir_from_cfg(launcher_root: &std::path::Path) -> Option<std::path::PathBuf> {
    for cfg_name in [
        "prismlauncher.cfg",
        "elyprismlauncher.cfg",
        "freesmlauncher.cfg",
    ] {
        let cfg_path = launcher_root.join(cfg_name);
        if let Ok(text) = std::fs::read_to_string(&cfg_path) {
            for line in text.lines() {
                if let Some(val) = line.trim().strip_prefix("InstanceDir=") {
                    let val = val.trim().trim_matches('"');
                    if !val.is_empty() && val != "instances" {
                        let p = std::path::PathBuf::from(val);
                        let resolved = if p.is_absolute() {
                            p
                        } else {
                            launcher_root.join(p)
                        };
                        if resolved.is_dir() {
                            return Some(resolved);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Where instances live: the launcher's own folder first, then PineconeMC (ElyPrismLauncher),
/// Prism Launcher, Freesm Launcher, Battly Launcher, OneClient, Lunar Client, and SKlauncher.
fn instance_base_dirs() -> Vec<std::path::PathBuf> {
    let appdata = std::path::PathBuf::from(std::env::var("APPDATA").unwrap_or_default());
    let local_appdata = std::path::PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default());
    let user_profile = std::path::PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default());

    let mut candidates = vec![
        // 1. IkaLauncher
        mc::instances_root(),
        // 2. PineconeMC / ElyPrismLauncher
        appdata.join("ElyPrismLauncher").join("instances"),
        appdata.join("PineconeMC").join("instances"),
        appdata.join("Pinecone").join("instances"),
        local_appdata
            .join("Programs")
            .join("ElyPrismLauncher")
            .join("instances"),
        local_appdata
            .join("Programs")
            .join("PineconeMC")
            .join("instances"),
        // 3. Prism Launcher
        appdata.join("PrismLauncher").join("instances"),
        local_appdata.join("PrismLauncher").join("instances"),
        local_appdata
            .join("Programs")
            .join("PrismLauncher")
            .join("instances"),
        // 4. Freesm Launcher
        appdata.join("FreesmLauncher").join("instances"),
        appdata.join("Freesm").join("instances"),
        appdata.join("freesm").join("instances"),
        local_appdata.join("FreesmLauncher").join("instances"),
        local_appdata
            .join("Programs")
            .join("FreesmLauncher")
            .join("instances"),
        local_appdata
            .join("Programs")
            .join("Freesm")
            .join("instances"),
        // 5. Battly Launcher
        appdata.join(".battly").join("instances"),
        appdata.join("BattlyLauncher").join("instances"),
        appdata.join("battly").join("instances"),
        appdata.join("instances"), // %APPDATA%\instances (used by Battly)
        // 6. OneClient
        appdata.join("Polyfrost").join("OneClient").join("clusters"),
        local_appdata.join("OneClient").join("clusters"),
        // 7. Lunar Client
        user_profile.join(".lunarclient").join("profiles"),
        // 8. SKlauncher
        appdata
            .join(".minecraft")
            .join("sklauncher")
            .join("instances"),
        appdata.join(".minecraft").join("profiles"),
        appdata.join(".minecraft").join("instances"),
        appdata.join("SKlauncher").join("instances"),
        appdata.join("sklauncher").join("instances"),
    ];

    for root_name in [
        "ElyPrismLauncher",
        "PineconeMC",
        "PrismLauncher",
        "FreesmLauncher",
        "Freesm",
    ] {
        if let Some(custom) = custom_instance_dir_from_cfg(&appdata.join(root_name)) {
            candidates.push(custom);
        }
    }

    // Also check profiles with custom gameDir in %APPDATA%\.minecraft\launcher_profiles.json
    let launcher_profiles = appdata.join(".minecraft").join("launcher_profiles.json");
    if launcher_profiles.is_file() {
        if let Ok(text) = std::fs::read_to_string(&launcher_profiles) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(profiles) = json.get("profiles").and_then(|p| p.as_object()) {
                    for (_id, p) in profiles {
                        if let Some(game_dir) = p.get("gameDir").and_then(|g| g.as_str()) {
                            let pb = std::path::PathBuf::from(game_dir);
                            if pb.is_dir() {
                                if let Some(parent) = pb.parent() {
                                    if parent.is_dir() {
                                        candidates.push(parent.to_path_buf());
                                    }
                                }
                                candidates.push(pb);
                            }
                        }
                    }
                }
            }
        }
    }

    let mut unique = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in candidates {
        let key = dir
            .to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase();
        if !key.is_empty() && seen.insert(key) {
            unique.push(dir);
        }
    }
    unique
}

fn canonical_loader_label(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    if lower.contains("neoforge") {
        "NeoForge".to_string()
    } else if lower.contains("forge") {
        "Forge".to_string()
    } else if lower.contains("fabric") {
        "Fabric".to_string()
    } else if lower.contains("quilt") {
        "Quilt".to_string()
    } else if lower.contains("optifine") {
        "OptiFine".to_string()
    } else if lower.is_empty() || lower == "vanilla" {
        "Vanilla".to_string()
    } else {
        raw.trim().to_string()
    }
}

fn file_mtime_ms(p: &std::path::Path) -> u64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Computes the timestamp (in milliseconds since UNIX_EPOCH) of the most recent activity
/// or modification of an instance (checking launch time, configs, mods, options, logs, saves).
fn entry_mtime_ms(e: &std::fs::DirEntry) -> u64 {
    e.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn instance_last_activity_ms(inst_dir: &std::path::Path) -> u64 {
    let mut best: u64 = 0;

    let cfg = inst_dir.join("instance.cfg");
    if cfg.is_file() {
        if let Ok(content) = std::fs::read_to_string(&cfg) {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(val) = trimmed.strip_prefix("lastLaunchTime=") {
                    if let Ok(ts) = val.trim().parse::<u64>() {
                        if ts > 0 {
                            let ms = if ts < 10_000_000_000 { ts * 1000 } else { ts };
                            best = best.max(ms);
                        }
                    }
                }
            }
        }
        best = best.max(file_mtime_ms(&cfg));
    }

    best = best.max(file_mtime_ms(&inst_dir.join("mmc-pack.json")));
    best = best.max(file_mtime_ms(&inst_dir.join("instance.json")));
    best = best.max(file_mtime_ms(
        &inst_dir.join(".oneclient-materialized.json"),
    ));
    best = best.max(file_mtime_ms(&inst_dir.join(".oneclient-mods.json")));
    best = best.max(file_mtime_ms(&inst_dir.join("base-modpack.mrpack")));
    best = best.max(file_mtime_ms(inst_dir));

    for sub in ["", "minecraft", ".minecraft"] {
        let g = if sub.is_empty() {
            inst_dir.to_path_buf()
        } else {
            inst_dir.join(sub)
        };
        best = best.max(file_mtime_ms(&g.join("options.txt")));
        best = best.max(file_mtime_ms(&g.join("logs").join("latest.log")));
        best = best.max(file_mtime_ms(&g.join("config")));
        best = best.max(file_mtime_ms(&g.join("saves")));

        let mdir = g.join("mods");
        if mdir.is_dir() {
            best = best.max(file_mtime_ms(&mdir));
            // The directory listing already carries each file's time and type (on Windows, no
            // extra disk access per file, unlike fs::metadata on its path).
            if let Ok(entries) = std::fs::read_dir(&mdir) {
                for e in entries.flatten() {
                    best = best.max(entry_mtime_ms(&e));
                    if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        if let Ok(sub_entries) = std::fs::read_dir(e.path()) {
                            for se in sub_entries.flatten() {
                                best = best.max(entry_mtime_ms(&se));
                            }
                        }
                    }
                }
            }
        }
    }

    best
}

/// Ensures external launcher instances (Battly, Lunar Client, OneClient, SKlauncher) also have
/// `instance.cfg` and `mmc-pack.json` so all launcher tools (Play, Edit, Share, Manager) work seamlessly.
fn ensure_external_prism_metadata(
    inst_dir: &std::path::Path,
    display_name: &str,
    mc_version: &str,
    loader: &str,
    loader_ver: &str,
) {
    let cfg_path = inst_dir.join("instance.cfg");
    if !cfg_path.exists() {
        let cfg = format!(
            "InstanceType=OneSix\nname={}\nIntendedVersion={}\nIntendedLoader={}\niconKey=default\nnotes=\n",
            display_name, mc_version, loader
        );
        let _ = std::fs::write(&cfg_path, cfg);
    }

    let pack_path = inst_dir.join("mmc-pack.json");
    if !pack_path.exists() && !mc_version.is_empty() {
        let mut components = vec![serde_json::json!({
            "cachedName": "Minecraft",
            "cachedVersion": mc_version,
            "important": true,
            "uid": "net.minecraft",
            "version": mc_version
        })];

        if let Some((uid, cached_name, _)) = mc::loader_uid_and_name(loader) {
            let mut clean_ver = loader_ver.trim().to_string();
            let prefix = format!("{}-", mc_version);
            if let Some(stripped) = clean_ver.strip_prefix(&prefix) {
                clean_ver = stripped.to_string();
            }
            if let Some(stripped) = clean_ver.strip_prefix(&format!("{}-", loader.to_lowercase())) {
                clean_ver = stripped.to_string();
            }
            if clean_ver.is_empty() {
                if let Some(fb) = mc::fallback_loader_version(uid, mc_version) {
                    clean_ver = fb;
                } else if uid.contains("fabric") {
                    clean_ver = "0.16.10".to_string();
                }
            }
            if !clean_ver.is_empty() {
                components.push(serde_json::json!({
                    "cachedName": cached_name,
                    "cachedVersion": clean_ver,
                    "uid": uid,
                    "version": clean_ver
                }));
            }
        }

        let pack_json = serde_json::json!({
            "components": components,
            "formatVersion": 1
        });
        let _ = std::fs::write(
            &pack_path,
            serde_json::to_string_pretty(&pack_json).unwrap_or_default(),
        );
    }
}

fn java_vendor(info: &str) -> &'static str {
    let info = info.to_ascii_lowercase();
    for (name, markers) in [
        ("Azul Zulu", &["zulu", "azul"][..]),
        ("Adoptium", &["adoptium", "temurin", "eclipse"]),
        ("Mojang", &["microsoft", "mojang", "java-runtime-"]),
        ("IBM Semeru", &["semeru", "openj9"]),
        ("JetBrains", &["jetbrains", "jbr", "jvmika"]),
        ("Corretto", &["corretto", "amazon"]),
        ("Liberica", &["liberica", "bellsoft"]),
        ("GraalVM", &["graalvm", "graal"]),
        ("Oracle", &["oracle", "jre-legacy"]),
    ] {
        if markers.iter().any(|marker| info.contains(marker)) {
            return name;
        }
    }
    "OpenJDK"
}

/// The instances of the WebView2 Home: the launcher folders only. Servers the
/// old Servers screen installed elsewhere (Cretania in `Lucerion/instancias`)
/// were never part of that list; `scan` still accepts them for the future
/// Servers screen.
pub(super) fn discover() -> Vec<LauncherInstance> {
    scan(&instance_base_dirs(), &[])
}

pub(super) fn scan(
    base_dirs: &[std::path::PathBuf],
    installed_servers: &[std::path::PathBuf],
) -> Vec<LauncherInstance> {
    let mut list: Vec<LauncherInstance> = Vec::new();
    let mut list_scores: Vec<(u64, u32, usize, usize, u64)> = Vec::new();
    let mut seen_paths: std::collections::HashSet<String> = std::collections::HashSet::new();

    let re_ver = regex::Regex::new(r"\b(?:1\.\d+(?:\.\d+)?|2[4-9]\.\d+(?:\.\d+)?)\b").ok();

    // The folders first (cheap), then every instance is read in parallel: config, modpack zip,
    // mod/pack counts, world statistics... One after another this took ~1.5 s at startup with ~70
    // instances. The results are merged in the original order, so the outcome is the same.
    let mut dirs: Vec<(std::path::PathBuf, String, String)> = Vec::new();
    for base in base_dirs {
        let base_path = base.as_path();
        if let Ok(entries) = std::fs::read_dir(base_path) {
            for entry in entries.flatten() {
                let inst_dir = entry.path();
                if !inst_dir.is_dir() {
                    continue;
                }
                let folder_name = entry.file_name().to_string_lossy().to_string();
                if folder_name.starts_with('.') || folder_name.starts_with('_') {
                    continue;
                }
                let norm_path_key = inst_dir
                    .to_string_lossy()
                    .replace('/', "\\")
                    .trim_end_matches('\\')
                    .to_lowercase();
                if seen_paths.insert(norm_path_key.clone()) {
                    dirs.push((inst_dir, folder_name, norm_path_key));
                }
            }
        }
    }

    for inst_dir in installed_servers {
        let key = inst_dir.to_string_lossy().replace('/', "\\").to_lowercase();
        if seen_paths.insert(key.clone()) {
            dirs.push((inst_dir.clone(), "Cretania".to_owned(), key));
        }
    }

    use rayon::prelude::*;
    let read = |(inst_dir, folder_name, norm_path_key): &(std::path::PathBuf, String, String)| -> Option<(LauncherInstance, (u64, u32, usize, usize, u64))> {
        let inst_dir = inst_dir.clone();
        let folder_name = folder_name.clone();
        let norm_path_key = norm_path_key.clone();
        let cfg_file = inst_dir.join("instance.cfg");
        let mmc_pack = inst_dir.join("mmc-pack.json");
        let battly_json = inst_dir.join("instance.json");

        let is_oneclient = inst_dir.to_string_lossy().to_lowercase().contains("oneclient");
        if is_oneclient {
            let is_materialized = inst_dir.join(".oneclient-materialized.json").is_file()
                || inst_dir.join(".oneclient-mods.json").is_file()
                || inst_dir.join("options.txt").is_file()
                || inst_dir.join("logs").is_dir()
                || inst_dir.join("instance.cfg").is_file()
                || inst_dir.join("mmc-pack.json").is_file();
            let has_jars = if inst_dir.join("mods").is_dir() {
                std::fs::read_dir(inst_dir.join("mods")).map(|rd| {
                    rd.flatten().any(|e| {
                        let n = e.file_name().to_string_lossy().to_lowercase();
                        (n.ends_with(".jar") || n.ends_with(".jar.disabled")) && !n.starts_with('.')
                    })
                }).unwrap_or(false)
            } else {
                false
            };
            if !is_materialized && !has_jars {
                return None;
            }
        }

        let is_lunar = inst_dir.to_string_lossy().to_lowercase().contains(".lunarclient");
        let has_lunar_meta = inst_dir.join("base-modpack.mrpack").is_file()
            || (is_lunar && inst_dir.join("mods").is_dir());

        // Skip folders that are not valid Minecraft instances
        let is_valid_instance = cfg_file.is_file()
            || mmc_pack.is_file()
            || battly_json.is_file()
            || has_lunar_meta
            || is_oneclient
            || inst_dir.join("minecraft").is_dir()
            || inst_dir.join(".minecraft").is_dir()
            || inst_dir.join("mods").is_dir();
        if !is_valid_instance {
            return None;
        }

        // Read name, iconKey, IntendedLoader, and JavaVersion from instance.cfg if available
        let mut display_name = folder_name.clone();
        let mut intended_version = String::new();
        let mut intended_loader = String::new();
        let mut icon_key = String::new();
        let mut java_version: Option<String> = None;
        let mut java_info = String::new();
        let mut total_time_played: Option<u64> = None;
        let mut max_mem_mb: Option<u32> = None;
        let mut external_loader_ver = String::new();

        if cfg_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&cfg_file) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if let Some((key, value)) = trimmed.split_once('=')
                        && matches!(key, "JavaPath" | "JavaVendor" | "JavaVersion")
                    {
                        java_info.push_str(value);
                        java_info.push(' ');
                    }
                    if trimmed.starts_with("name=") {
                        let val = trimmed["name=".len()..].trim();
                        if !val.is_empty() {
                            display_name = val.to_string();
                        }
                    } else if trimmed.starts_with("iconKey=") {
                        icon_key = trimmed["iconKey=".len()..].trim().to_string();
                    } else if trimmed.starts_with("IntendedVersion=") {
                        intended_version = trimmed["IntendedVersion=".len()..].trim().to_string();
                    } else if trimmed.starts_with("IntendedLoader=") {
                        intended_loader = trimmed["IntendedLoader=".len()..].trim().to_string();
                    } else if trimmed.starts_with("totalTimePlayed=") {
                        let t = trimmed["totalTimePlayed=".len()..].trim();
                        if let Ok(secs) = t.parse::<u64>() {
                            total_time_played = Some(secs);
                        }
                    } else if let Some(m) = trimmed.strip_prefix("MaxMemAlloc=") {
                        if let Ok(mb) = m.trim().parse::<u32>()
                            && mb > 0
                        {
                            max_mem_mb = Some(mb);
                        }
                    } else if trimmed.starts_with("IkaJavaMajor=") {
                        let jm = trimmed["IkaJavaMajor=".len()..].trim();
                        if let Ok(num) = jm.parse::<u32>() {
                            java_version = Some(format!("Java {}", num));
                        }
                    } else if trimmed.starts_with("JavaVersion=") {
                        let jv = trimmed["JavaVersion=".len()..].trim();
                        if !jv.is_empty() {
                            if jv.starts_with("1.8") {
                                java_version = Some("Java 8".to_string());
                            } else if let Some(major) = jv.split('.').next() {
                                if let Ok(num) = major.parse::<u32>() {
                                    java_version = Some(format!("Java {}", num));
                                } else {
                                    java_version = Some(format!("Java {}", jv));
                                }
                            } else {
                                java_version = Some(format!("Java {}", jv));
                            }
                        }
                    }
                }
            }
        }

        // Extract mc_version & loader from mmc-pack.json
        let mut mc_version = String::new();
        let mut loader = String::new();
        let mut loader_version: Option<String> = None;

        if mmc_pack.exists() {
            if let Ok(content) = std::fs::read_to_string(&mmc_pack) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(components) = json.get("components").and_then(|c| c.as_array()) {
                        for comp in components {
                            let uid = comp.get("uid").and_then(|u| u.as_str()).unwrap_or("");
                            let ver = comp
                                .get("version")
                                .or_else(|| comp.get("cachedVersion"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let name = comp.get("cachedName").and_then(|n| n.as_str()).unwrap_or("");

                            if uid == "net.minecraft" {
                                if !ver.is_empty() {
                                    mc_version = ver.to_string();
                                }
                            } else if uid.contains("minecraftforge") || uid == "net.minecraftforge" {
                                loader = "Forge".to_string();
                                if !ver.is_empty() {
                                    loader_version = Some(ver.to_string());
                                }
                            } else if uid.contains("neoforge") || uid == "net.neoforged" {
                                loader = "NeoForge".to_string();
                                if !ver.is_empty() {
                                    loader_version = Some(ver.to_string());
                                }
                            } else if uid.contains("fabric") || uid == "net.fabricmc.fabric-loader" {
                                loader = "Fabric".to_string();
                                if !ver.is_empty() {
                                    loader_version = Some(ver.to_string());
                                }
                            } else if uid.contains("quilt") || uid == "org.quiltmc.quilt-loader" {
                                loader = "Quilt".to_string();
                                if !ver.is_empty() {
                                    loader_version = Some(ver.to_string());
                                }
                            } else if !name.is_empty()
                                && (name.eq_ignore_ascii_case("forge")
                                    || name.eq_ignore_ascii_case("fabric")
                                    || name.eq_ignore_ascii_case("neoforge")
                                    || name.eq_ignore_ascii_case("quilt"))
                            {
                                loader = canonical_loader_label(name);
                                if !ver.is_empty() {
                                    loader_version = Some(ver.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Lunar Client base-modpack.mrpack support
        if is_lunar {
            let mrpack = inst_dir.join("base-modpack.mrpack");
            if mrpack.is_file() {
                if let Ok(file) = std::fs::File::open(&mrpack) {
                    if let Ok(mut zip) = zip::ZipArchive::new(file) {
                        if let Ok(mut idx_file) = zip.by_name("modrinth.index.json") {
                            let mut s = String::new();
                            if std::io::Read::read_to_string(&mut idx_file, &mut s).is_ok() {
                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&s) {
                                    if let Some(deps) = val.get("dependencies").and_then(|d| d.as_object()) {
                                        if let Some(mv) = deps.get("minecraft").and_then(|v| v.as_str()) {
                                            if mc_version.is_empty() || mc_version == "1.20.1" {
                                                mc_version = mv.to_string();
                                            }
                                        }
                                        for (k, v) in deps {
                                            if k.contains("fabric") {
                                                loader = "Fabric".to_string();
                                                if let Some(ver) = v.as_str() {
                                                    external_loader_ver = ver.to_string();
                                                }
                                            } else if k.contains("forge") && !k.contains("neoforge") {
                                                loader = "Forge".to_string();
                                                if let Some(ver) = v.as_str() {
                                                    external_loader_ver = ver.to_string();
                                                }
                                            } else if k.contains("neoforge") {
                                                loader = "NeoForge".to_string();
                                                if let Some(ver) = v.as_str() {
                                                    external_loader_ver = ver.to_string();
                                                }
                                            } else if k.contains("quilt") {
                                                loader = "Quilt".to_string();
                                                if let Some(ver) = v.as_str() {
                                                    external_loader_ver = ver.to_string();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if (display_name == folder_name || display_name.is_empty()) && folder_name.chars().all(|c| c.is_ascii_digit()) {
                display_name = format!("Minecraft {}", folder_name);
            }
        }

        // OneClient cluster loader fallback from folder name
        if is_oneclient && (loader.is_empty() || loader == "Vanilla") {
            let lower = folder_name.to_lowercase();
            if lower.contains("fabric") {
                loader = "Fabric".to_string();
            } else if lower.contains("neoforge") {
                loader = "NeoForge".to_string();
            } else if lower.contains("forge") {
                loader = "Forge".to_string();
            } else if lower.contains("quilt") {
                loader = "Quilt".to_string();
            }
        }

        // Battly Launcher instance.json support
        let mut battly_custom_icon: Option<String> = None;
        if battly_json.is_file() {
            if let Ok(content) = std::fs::read_to_string(&battly_json) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(b_name) = json.get("name").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) {
                        if display_name == folder_name {
                            display_name = b_name.to_string();
                        }
                    }
                    let b_ver = json.get("version").and_then(|v| v.as_str()).map(str::trim).unwrap_or("");
                    if mc_version.is_empty() && !b_ver.is_empty() {
                        mc_version = b_ver.to_string();
                    }
                    let b_loader = json.get("loader").and_then(|v| v.as_str()).map(str::trim).unwrap_or("");
                    if loader.is_empty() && !b_loader.is_empty() {
                        loader = canonical_loader_label(b_loader);
                    }
                    let b_loader_ver = json.get("loaderVersion").and_then(|v| v.as_str()).map(str::trim).unwrap_or("");
                    if !b_loader_ver.is_empty() {
                        external_loader_ver = b_loader_ver.to_string();
                    }
                    if let Some(img) = json.get("image").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) {
                        let img_path = std::path::PathBuf::from(img.replace('/', "\\"));
                        // Skip Battly's default 94,246-byte grass block icon so our unique block faces apply
                        if let Ok(meta) = std::fs::metadata(&img_path) {
                            if meta.is_file() && meta.len() != 94246 {
                                battly_custom_icon = Some(img_path.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }

        // Fallback version check
        if mc_version.is_empty() && !intended_version.is_empty() {
            mc_version = intended_version;
        }
        if mc_version.is_empty() {
            if let Some(re) = &re_ver {
                if let Some(m) = re.find(&display_name).or_else(|| re.find(&folder_name)) {
                    mc_version = m.as_str().to_string();
                }
            }
        }
        if mc_version.is_empty() {
            mc_version = "1.20.1".to_string();
        }

        // Fallback loader check
        if loader.is_empty() && !intended_loader.is_empty() {
            loader = canonical_loader_label(&intended_loader);
        }
        if loader.is_empty() {
            let lower = format!("{} {}", display_name.to_lowercase(), folder_name.to_lowercase());
            if lower.contains("neoforge") {
                loader = "NeoForge".to_string();
            } else if lower.contains("forge") {
                loader = "Forge".to_string();
            } else if lower.contains("fabric") {
                loader = "Fabric".to_string();
            } else if lower.contains("quilt") {
                loader = "Quilt".to_string();
            } else {
                loader = "Vanilla".to_string();
            }
        }

        if loader_version.is_none() && !external_loader_ver.is_empty() {
            loader_version = Some(external_loader_ver.clone());
        }


        // Fallback java_version check if not present in instance.cfg
        if java_version.is_none() && mc_version.starts_with("26.") {
            java_version = Some("Java 25".to_string());
        }
        if java_version.is_none() {
            if mc_version.starts_with("1.21") || mc_version.starts_with("1.20.5") || mc_version.starts_with("1.20.6")  {
                java_version = Some("Java 21".to_string());
            } else if mc_version.starts_with("1.16")
                || mc_version.starts_with("1.15")
                || mc_version.starts_with("1.14")
                || mc_version.starts_with("1.13")
                || mc_version.starts_with("1.12")
                || mc_version.starts_with("1.8")
                || mc_version.starts_with("1.7")
                || mc_version.starts_with("1.5")
            {
                java_version = Some("Java 8".to_string());
            } else {
                java_version = Some("Java 17".to_string());
            }
        }

        // Find icon or image in launcher icons or instance directory
        let native_icon = inst_dir.join("ika-icon.png");
        let mut icon_path = native_icon.is_file().then(|| native_icon.to_string_lossy().into_owned()).or(battly_custom_icon);
        let exts = ["webp", "png", "jpg", "jpeg"];

        if icon_path.is_none() && !icon_key.is_empty() && icon_key != "default" {
            let icon_dirs: Vec<std::path::PathBuf> = base_dirs
                .iter()
                .filter_map(|b| b.parent().map(|p| p.join("icons")))
                .collect();
            for idir in &icon_dirs {
                for ext in &exts {
                    let candidate = idir.join(format!("{}.{}", icon_key, ext));
                    if candidate.exists() {
                        icon_path = Some(candidate.to_string_lossy().to_string());
                        break;
                    }
                }
                if icon_path.is_some() {
                    break;
                }
            }
        }

        // Check inside instance directory for icon_key file or icon.png
        if icon_path.is_none() && !icon_key.is_empty() && icon_key != "default" {
            for ext in &exts {
                let candidate = inst_dir.join(format!("{}.{}", icon_key, ext));
                if candidate.exists() {
                    icon_path = Some(candidate.to_string_lossy().to_string());
                    break;
                }
            }
        }

        if icon_path.is_none() {
            let icon_png = inst_dir.join("icon.png");
            if let Ok(meta) = std::fs::metadata(&icon_png) {
                // Skip Battly's default 94,246-byte grass block icon so our Minecraft block faces are used
                let is_battly_default = battly_json.is_file() && meta.len() == 94246;
                if meta.is_file() && !is_battly_default {
                    icon_path = Some(icon_png.to_string_lossy().to_string());
                }
            }
        }

        if icon_path.is_none() {
            if let Ok(inst_files) = std::fs::read_dir(&inst_dir) {
                for f in inst_files.flatten() {
                    let p = f.path();
                    if let Some(ext) = p.extension().and_then(|x| x.to_str()) {
                        let ext_low = ext.to_lowercase();
                        if ext_low == "webp" || ext_low == "png" || ext_low == "jpg" || ext_low == "jpeg" {
                            let fname = f.file_name().to_string_lossy().to_lowercase();
                            if fname == "icon.png" && battly_json.is_file() {
                                continue;
                            }
                            if fname.starts_with("modrinth_")
                                || fname.starts_with("curseforge_")
                                || fname.contains("icon")
                                || fname.contains("logo")
                            {
                                icon_path = Some(p.to_string_lossy().to_string());
                                break;
                            }
                        }
                    }
                }
            }
        }

        // Check mods dir (including nested loader subfolders such as Lunar Client mods/fabric-26.2)
        let mc_mods = inst_dir.join("minecraft").join("mods");
        let dot_mc_mods = inst_dir.join(".minecraft").join("mods");
        let root_mods = inst_dir.join("mods");

        let mut mods_dir = if mc_mods.exists() {
            mc_mods
        } else if dot_mc_mods.exists() {
            dot_mc_mods
        } else if root_mods.exists() || battly_json.is_file() || is_oneclient || is_lunar {
            root_mods
        } else if inst_dir.join(".minecraft").exists() || inst_dir.to_string_lossy().contains("IkaLauncher") {
            dot_mc_mods
        } else {
            mc_mods
        };

        if mods_dir.is_dir() {
            let has_direct_jars = std::fs::read_dir(&mods_dir)
                .map(|rd| {
                    rd.flatten().any(|e| {
                        let name = e.file_name().to_string_lossy().to_lowercase();
                        (name.ends_with(".jar") || name.ends_with(".jar.disabled")) && !name.starts_with('.')
                    })
                })
                .unwrap_or(false);

            if !has_direct_jars {
                if let Ok(entries) = std::fs::read_dir(&mods_dir) {
                    for e in entries.flatten() {
                        let p = e.path();
                        if p.is_dir() {
                            let has_sub_jars = std::fs::read_dir(&p)
                                .map(|rd| {
                                    rd.flatten().any(|se| {
                                        let name = se.file_name().to_string_lossy().to_lowercase();
                                        (name.ends_with(".jar") || name.ends_with(".jar.disabled")) && !name.starts_with('.')
                                    })
                                })
                                .unwrap_or(false);
                            if has_sub_jars {
                                mods_dir = p;
                                break;
                            }
                        }
                    }
                }
            }
        }

        let mod_count = if mods_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&mods_dir) {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        let name = e.file_name().to_string_lossy().to_lowercase();
                        (name.ends_with(".jar") || name.ends_with(".jar.disabled"))
                            && !name.starts_with('.')
                    })
                    .count()
            } else {
                0
            }
        } else {
            0
        };

        // Locate base minecraft folder for shaderpacks, resourcepacks, saves, etc.
        let game_dir = if inst_dir.join("minecraft").exists() {
            inst_dir.join("minecraft")
        } else if inst_dir.join(".minecraft").exists() {
            inst_dir.join(".minecraft")
        } else {
            inst_dir.clone()
        };

        // Fallback for total_time_played if 0 or not found: check saves/ world statistics
        if total_time_played.unwrap_or(0) == 0 {
            let saves_dir = game_dir.join("saves");
            if saves_dir.exists() {
                if let Ok(worlds) = std::fs::read_dir(saves_dir) {
                    let mut total_ticks: u64 = 0;
                    for w in worlds.flatten() {
                        let stats_dir = w.path().join("stats");
                        if stats_dir.exists() {
                            if let Ok(stat_files) = std::fs::read_dir(stats_dir) {
                                for sf in stat_files.flatten() {
                                    let p = sf.path();
                                    if p.extension().map_or(false, |ext| ext == "json") {
                                        if let Ok(data) = std::fs::read_to_string(&p) {
                                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                                                if let Some(custom) = v.get("stats").and_then(|s| s.get("minecraft:custom")) {
                                                    if let Some(ticks) = custom
                                                        .get("minecraft:play_time")
                                                        .or_else(|| custom.get("minecraft:play_one_minute"))
                                                        .and_then(|t| t.as_u64())
                                                    {
                                                        total_ticks += ticks;
                                                    }
                                                } else if let Some(stat_old) = v.get("stat.playOneMinute").and_then(|t| t.as_u64()) {
                                                    total_ticks += stat_old;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if total_ticks > 0 {
                        total_time_played = Some(total_ticks / 20);
                    }
                }
            }
        }

        let count_packs_in = |sub: &str| -> usize {
            let candidates = [
                game_dir.join(sub),
                inst_dir.join("minecraft").join(sub),
                inst_dir.join(".minecraft").join(sub),
                inst_dir.join(sub),
            ];
            let mut best = 0;
            for dir in &candidates {
                if !dir.is_dir() {
                    continue;
                }
                if let Ok(entries) = std::fs::read_dir(dir) {
                    let mut c = 0;
                    for e in entries.flatten() {
                        let p = e.path();
                        let name = e.file_name().to_string_lossy().to_string();
                        let name_low = name.to_lowercase();
                        if name.starts_with('.')
                            || name_low.ends_with(".txt")
                            || name_low.ends_with(".rpo")
                            || name_low.ends_with(".properties")
                            || name_low.ends_with(".json")
                            || name_low.ends_with(".toml")
                            || name_low.ends_with(".disabled.txt")
                        {
                            continue;
                        }
                        let is_archive = name_low.ends_with(".zip")
                            || name_low.ends_with(".zip.disabled")
                            || name_low.ends_with(".jar")
                            || name_low.ends_with(".jar.disabled")
                            || name_low.ends_with(".7z")
                            || name_low.ends_with(".rar");
                        if p.is_dir() || is_archive {
                            c += 1;
                        }
                    }
                    if c > best {
                        best = c;
                    }
                }
            }
            best
        };

        // Shader packs count
        let shader_count = count_packs_in("shaderpacks");

        // Resource packs count (check resourcepacks and legacy texturepacks)
        let resourcepack_count = count_packs_in("resourcepacks").max(count_packs_in("texturepacks"));

        let candidate = LauncherInstance {
            name: display_name.trim().to_string(),
            path: inst_dir.to_string_lossy().to_string(),
            mod_count,
            mc_version,
            loader,
            loader_version,
            icon_path,
            java_version,
            java_vendor: java_vendor(&java_info),
            total_time_played,
            shader_count,
            resourcepack_count,
            max_mem_mb: max_mem_mb.unwrap_or(8192),
        };


        // Deduplication scoring:
        // 1. inst_mtime: Most recently modified, configured, or played instance ALWAYS wins
        // 2. launcher_priority: Ties prefer PineconeMC/ElyPrism > Freesm > OneClient/Lunar/SK/Battly > older Prism
        // 3. mod_count & packs_count & total_time_played break any further ties
        let inst_mtime = instance_last_activity_ms(&inst_dir);
        let norm_path = norm_path_key.to_lowercase();
        let launcher_priority: u32 = if norm_path.contains("ikalau") {
            5
        } else if norm_path.contains("elyprism") || norm_path.contains("pinecone") {
            4
        } else if norm_path.contains("freesm") {
            3
        } else if norm_path.contains("oneclient") || norm_path.contains("lunar") || norm_path.contains("sklauncher") || norm_path.contains("battly") {
            2
        } else {
            1 // Older Prism
        };

        let score = (
            inst_mtime,
            launcher_priority,
            candidate.mod_count,
            candidate.shader_count + candidate.resourcepack_count,
            candidate.total_time_played.unwrap_or(0),
        );

        Some((candidate, score))
    };
    let read_all: Vec<_> = dirs.par_iter().map(read).collect();

    let norm_key = |s: &str| -> String {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    for (candidate, score) in read_all.into_iter().flatten() {
        let name_key = norm_key(&candidate.name);
        if let Some(dup_idx) = list.iter().position(|x| norm_key(&x.name) == name_key) {
            if score > list_scores[dup_idx] {
                list[dup_idx] = candidate;
                list_scores[dup_idx] = score;
            }
            continue;
        }

        list.push(candidate);
        list_scores.push(score);
    }

    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list
}

pub(super) fn prepare(instance: &LauncherInstance) -> Result<(), String> {
    let path = std::path::Path::new(&instance.path);
    if !path.is_dir() {
        return Err("Ya no existe la carpeta de la instancia.".into());
    }
    ensure_external_prism_metadata(
        path,
        &instance.name,
        &instance.mc_version,
        &instance.loader,
        instance.loader_version.as_deref().unwrap_or(""),
    );
    Ok(())
}
