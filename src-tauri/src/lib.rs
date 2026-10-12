mod knowledge;
mod mc;
mod skin_server;
mod device_key;
mod skinrestorer_setup;
mod console_tools;
mod disk_cache;
mod scanner;
mod manager;
mod share;

use scanner::{ComparisonOutput, ModInfo, ScanOutput};
use std::io::{BufRead, BufReader};
use std::sync::Mutex;
use tauri::Emitter;

static STEAM_DOWNLOAD_LOCK: Mutex<()> = Mutex::new(());

#[tauri::command]
fn pick_folder() -> Option<String> {
    rfd::FileDialog::new()
        .set_title("Seleccionar carpeta de la instancia de Minecraft")
        .pick_folder()
        .map(|p| p.to_string_lossy().to_string())
}

#[tauri::command(async)]
fn scan_directory(path: String) -> ScanOutput {
    scanner::scan_mods_in_dir(&path)
}

#[tauri::command(async)]
fn compare_instances(mods1: Vec<ModInfo>, mods2: Vec<ModInfo>) -> ComparisonOutput {
    scanner::compare(&mods1, &mods2)
}

#[tauri::command(async)]
fn open_browser(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("Ruta vacía".to_string());
    }
    let lower = trimmed.to_lowercase();
    let is_scheme = lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("steam://");

    if !is_scheme {
        let normalized = trimmed.replace('/', "\\");
        let path = std::path::PathBuf::from(&normalized);
        let target = if path.exists() {
            path
        } else {
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let parent = path.parent();
            if matches!(file_name.as_str(), "minecraft" | ".minecraft") {
                if let Some(p) = parent {
                    if p.join(".minecraft").exists() {
                        p.join(".minecraft")
                    } else if p.join("minecraft").exists() {
                        p.join("minecraft")
                    } else if p.exists() {
                        p.to_path_buf()
                    } else {
                        path
                    }
                } else {
                    path
                }
            } else if matches!(
                file_name.as_str(),
                "mods" | "shaderpacks" | "resourcepacks" | "screenshots" | "config" | "saves"
            ) {
                if let Some(p) = parent {
                    let p_name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().to_lowercase())
                        .unwrap_or_default();
                    let actual_game_dir = if p.exists() {
                        p.to_path_buf()
                    } else if matches!(p_name.as_str(), "minecraft" | ".minecraft") {
                        if let Some(inst_root) = p.parent() {
                            if inst_root.join(".minecraft").exists() {
                                inst_root.join(".minecraft")
                            } else if inst_root.join("minecraft").exists() {
                                inst_root.join("minecraft")
                            } else if inst_root.join("instance.json").exists() {
                                inst_root.to_path_buf()
                            } else if inst_root.exists() {
                                let g = if inst_root.to_string_lossy().contains("IkaLauncher") {
                                    inst_root.join(".minecraft")
                                } else {
                                    inst_root.join("minecraft")
                                };
                                let _ = std::fs::create_dir_all(&g);
                                g
                            } else {
                                p.to_path_buf()
                            }
                        } else {
                            p.to_path_buf()
                        }
                    } else {
                        p.to_path_buf()
                    };
                    let sub = actual_game_dir.join(&file_name);
                    let _ = std::fs::create_dir_all(&sub);
                    if sub.exists() {
                        sub
                    } else if actual_game_dir.exists() {
                        actual_game_dir
                    } else {
                        path
                    }
                } else {
                    path
                }
            } else if let Some(anc) = path
                .ancestors()
                .skip(1)
                .find(|a| a.exists() && a.parent().is_some())
            {
                anc.to_path_buf()
            } else {
                path
            }
        };

        #[cfg(target_os = "windows")]
        if target.is_dir() {
            if std::process::Command::new("explorer.exe")
                .arg(&target)
                .spawn()
                .is_ok()
            {
                return Ok(());
            }
        }

        return open::that(&target).map_err(|e| e.to_string());
    }

    open::that(trimmed).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn toggle_mod_disabled(file_path: String) -> Result<ModInfo, String> {
    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return Err("El archivo no existe en la ruta especificada.".to_string());
    }

    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "Nombre de archivo no válido.".to_string())?;

    let parent = path
        .parent()
        .ok_or_else(|| "No se pudo determinar el directorio contenedor.".to_string())?;

    let new_path = if file_name.to_lowercase().ends_with(".disabled") {
        let stripped = &file_name[..file_name.len() - 9];
        parent.join(stripped)
    } else {
        parent.join(format!("{}.disabled", file_name))
    };

    std::fs::rename(path, &new_path).map_err(|e| format!("Error al renombrar mod: {}", e))?;
    let mut updated = scanner::parse_mod_file(&new_path);
    scanner::apply_user_categories(std::slice::from_mut(&mut updated));
    Ok(updated)
}

#[tauri::command(async)]
fn delete_mod_file(file_path: String) -> Result<(), String> {
    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return Err("El archivo ya no existe en el disco.".to_string());
    }
    std::fs::remove_file(path).map_err(|e| format!("Error al eliminar el mod: {}", e))?;
    Ok(())
}

#[tauri::command(async)]
fn get_mod_details(
    mod_id: String,
    slug: String,
    name: String,
    raw_desc: String,
    category: String,
) -> knowledge::DetailedModPayload {
    knowledge::get_mod_detailed_info(&mod_id, &slug, &name, &raw_desc, &category)
}

/// Downloads whatever the instance is missing and starts Minecraft directly with an
/// offline account (see `mc.rs`); no Prism/Pinecone install needed.
/// Shared skins (Ika skins service + SkinRestorer): URL of the service, or None while it
/// isn't deployed yet.
#[tauri::command(async)]
fn skins_service_status() -> Option<String> {
    device_key::skins_service_url()
}

fn skins_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout_read(std::time::Duration::from_secs(60))
        .user_agent("IkaLauncher/1.0")
        .build()
}

/// Installs and configures SkinRestorer in a dedicated server folder (version and loader
/// detected from its files).
#[tauri::command(async)]
fn prepare_server_skins(folder: String) -> Result<String, String> {
    let service = device_key::skins_service_url().ok_or("Primero hay que publicar el servicio de skins")?;
    let root = std::path::PathBuf::from(&folder);
    let (mc, loader) = skinrestorer_setup::detect_server(&root)?;
    let done = skinrestorer_setup::install_into(&skins_agent(), &root, &mc, &loader, &service)?;
    Ok(format!("Servidor {} {}: {}. Reinícialo para aplicar los cambios.", loader, mc, done))
}

/// Folder with SkinRestorer + config ready to upload to a hosting panel; opens it.
#[tauri::command(async)]
fn export_skins_for_hosting(mc_version: String, loader: String) -> Result<String, String> {
    let service = device_key::skins_service_url().ok_or("Primero hay que publicar el servicio de skins")?;
    let loader = loader.trim().to_lowercase();
    if !["fabric", "quilt", "forge", "neoforge"].contains(&loader.as_str()) {
        return Err("Loader no soportado".to_string());
    }
    let out = skinrestorer_setup::export_for_hosting(&skins_agent(), mc_version.trim(), &loader, &service)?;
    let _ = open::that(&out);
    Ok(out.to_string_lossy().to_string())
}

/// Skin of the active profile as sent by the UI (PNG in base64).
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSkin {
    png_base64: String,
    slim: bool,
}

#[tauri::command]
async fn launch_game(
    app: tauri::AppHandle,
    instance_path: String,
    username: String,
    skin: Option<LaunchSkin>,
) -> Result<String, String> {
    use base64::Engine;
    let skin = skin.and_then(|s| {
        base64::engine::general_purpose::STANDARD
            .decode(s.png_base64.trim())
            .ok()
            .filter(|png| png.len() <= 2 * 1024 * 1024)
            .map(|png| mc::SkinInput { png, slim: s.slim })
    });
    tauri::async_runtime::spawn_blocking(move || mc::launch_instance(app, instance_path, username, skin))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct LauncherInstance {
    pub name: String,
    pub folder_name: String,
    pub path: String,
    pub mods_path: String,
    pub mod_count: usize,
    pub mc_version: String,
    pub loader: String,
    pub icon_path: Option<String>,
    pub java_version: Option<String>,
    pub java_vendor: Option<String>,
    pub java_path: Option<String>,
    pub total_time_played: Option<u64>,
    pub shader_count: usize,
    pub resourcepack_count: usize,
    pub max_mem_mb: Option<u32>,
}

/// Reads custom `InstanceDir=...` from a Prism/Freesm/ElyPrism configuration file if set.
fn custom_instance_dir_from_cfg(launcher_root: &std::path::Path) -> Option<std::path::PathBuf> {
    for cfg_name in ["prismlauncher.cfg", "elyprismlauncher.cfg", "freesmlauncher.cfg"] {
        let cfg_path = launcher_root.join(cfg_name);
        if let Ok(text) = std::fs::read_to_string(&cfg_path) {
            for line in text.lines() {
                if let Some(val) = line.trim().strip_prefix("InstanceDir=") {
                    let val = val.trim().trim_matches('"');
                    if !val.is_empty() && val != "instances" {
                        let p = std::path::PathBuf::from(val);
                        let resolved = if p.is_absolute() { p } else { launcher_root.join(p) };
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
        local_appdata.join("Programs").join("ElyPrismLauncher").join("instances"),
        local_appdata.join("Programs").join("PineconeMC").join("instances"),
        // 3. Prism Launcher
        appdata.join("PrismLauncher").join("instances"),
        local_appdata.join("PrismLauncher").join("instances"),
        local_appdata.join("Programs").join("PrismLauncher").join("instances"),
        // 4. Freesm Launcher
        appdata.join("FreesmLauncher").join("instances"),
        appdata.join("Freesm").join("instances"),
        appdata.join("freesm").join("instances"),
        local_appdata.join("FreesmLauncher").join("instances"),
        local_appdata.join("Programs").join("FreesmLauncher").join("instances"),
        local_appdata.join("Programs").join("Freesm").join("instances"),
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
        appdata.join(".minecraft").join("sklauncher").join("instances"),
        appdata.join(".minecraft").join("profiles"),
        appdata.join(".minecraft").join("instances"),
        appdata.join("SKlauncher").join("instances"),
        appdata.join("sklauncher").join("instances"),
    ];

    for root_name in ["ElyPrismLauncher", "PineconeMC", "PrismLauncher", "FreesmLauncher", "Freesm"] {
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
    best = best.max(file_mtime_ms(&inst_dir.join(".oneclient-materialized.json")));
    best = best.max(file_mtime_ms(&inst_dir.join(".oneclient-mods.json")));
    best = best.max(file_mtime_ms(&inst_dir.join("base-modpack.mrpack")));
    best = best.max(file_mtime_ms(inst_dir));

    for sub in ["", "minecraft", ".minecraft"] {
        let g = if sub.is_empty() { inst_dir.to_path_buf() } else { inst_dir.join(sub) };
        best = best.max(file_mtime_ms(&g.join("options.txt")));
        best = best.max(file_mtime_ms(&g.join("logs").join("latest.log")));
        best = best.max(file_mtime_ms(&g.join("config")));
        best = best.max(file_mtime_ms(&g.join("saves")));

        let mdir = g.join("mods");
        if mdir.is_dir() {
            best = best.max(file_mtime_ms(&mdir));
            if let Ok(entries) = std::fs::read_dir(&mdir) {
                for e in entries.flatten() {
                    let ep = e.path();
                    best = best.max(file_mtime_ms(&ep));
                    if ep.is_dir() {
                        if let Ok(sub_entries) = std::fs::read_dir(&ep) {
                            for se in sub_entries.flatten() {
                                best = best.max(file_mtime_ms(&se.path()));
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
        let _ = std::fs::write(&pack_path, serde_json::to_string_pretty(&pack_json).unwrap_or_default());
    }
}

#[tauri::command(async)]
fn get_all_instances() -> Vec<LauncherInstance> {
    let mut list: Vec<LauncherInstance> = Vec::new();
    let mut list_scores: Vec<(u64, u32, usize, usize, u64)> = Vec::new();
    let mut seen_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    let base_dirs = instance_base_dirs();

    let re_ver = regex::Regex::new(r"\b(?:1\.\d+(?:\.\d+)?|2[4-9]\.\d+(?:\.\d+)?)\b").ok();

    for base in &base_dirs {
        let base_path = base.as_path();
        if base_path.exists() {
            if let Ok(entries) = std::fs::read_dir(base_path) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        let inst_dir = entry.path();
                        let folder_name = entry.file_name().to_string_lossy().to_string();
                        if folder_name.starts_with('.') || folder_name.starts_with('_') {
                            continue;
                        }

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
                                continue;
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
                            continue;
                        }

                        let norm_path_key = inst_dir
                            .to_string_lossy()
                            .replace('/', "\\")
                            .trim_end_matches('\\')
                            .to_lowercase();
                        if !seen_paths.insert(norm_path_key.clone()) {
                            continue;
                        }

                        // Read name, iconKey, IntendedLoader, and JavaVersion from instance.cfg if available
                        let mut display_name = folder_name.clone();
                        let mut intended_version = String::new();
                        let mut intended_loader = String::new();
                        let mut icon_key = String::new();
                        let mut java_version: Option<String> = None;
                        let mut java_vendor: Option<String> = None;
                        let mut java_path: Option<String> = None;
                        let mut total_time_played: Option<u64> = None;
                        let mut max_mem_mb: Option<u32> = None;
                        let mut external_loader_ver = String::new();

                        if cfg_file.exists() {
                            if let Ok(content) = std::fs::read_to_string(&cfg_file) {
                                for line in content.lines() {
                                    let trimmed = line.trim();
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
                                    } else if trimmed.starts_with("MaxMemAlloc=") {
                                        let m = trimmed["MaxMemAlloc=".len()..].trim();
                                        if let Ok(mb) = m.parse::<u32>() {
                                            if mb > 0 {
                                                max_mem_mb = Some(mb);
                                            }
                                        }
                                    } else if trimmed.starts_with("JavaVendor=") || trimmed.starts_with("IkaJavaVendor=") {
                                        let jv = trimmed.split_once('=').map(|(_, v)| v.trim()).unwrap_or("");
                                        if !jv.is_empty() {
                                            java_vendor = Some(jv.to_string());
                                        }
                                    } else if trimmed.starts_with("JavaPath=") {
                                        let jp = trimmed["JavaPath=".len()..].trim();
                                        if !jp.is_empty() {
                                            java_path = Some(jp.to_string());
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
                                            } else if uid.contains("neoforge") || uid == "net.neoforged" {
                                                loader = "NeoForge".to_string();
                                            } else if uid.contains("fabric") || uid == "net.fabricmc.fabric-loader" {
                                                loader = "Fabric".to_string();
                                            } else if uid.contains("quilt") || uid == "org.quiltmc.quilt-loader" {
                                                loader = "Quilt".to_string();
                                            } else if !name.is_empty()
                                                && (name.eq_ignore_ascii_case("forge")
                                                    || name.eq_ignore_ascii_case("fabric")
                                                    || name.eq_ignore_ascii_case("neoforge")
                                                    || name.eq_ignore_ascii_case("quilt"))
                                            {
                                                loader = canonical_loader_label(name);
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

                        // Synthesize missing Prism metadata for external launcher instances (Battly, Lunar, OneClient, SKlauncher)
                        ensure_external_prism_metadata(&inst_dir, &display_name, &mc_version, &loader, &external_loader_ver);

                        // Fallback java_version check if not present in instance.cfg
                        if java_version.is_none() {
                            if mc_version.starts_with("1.21") || mc_version.starts_with("1.20.5") || mc_version.starts_with("1.20.6") || mc_version.starts_with("26.") {
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
                        let mut icon_path: Option<String> = battly_custom_icon;
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
                            folder_name,
                            path: inst_dir.to_string_lossy().to_string(),
                            mods_path: mods_dir.to_string_lossy().to_string(),
                            mod_count,
                            mc_version,
                            loader,
                            icon_path,
                            java_version,
                            java_vendor,
                            java_path,
                            total_time_played,
                            shader_count,
                            resourcepack_count,
                            max_mem_mb,
                        };

                        let norm_key = |s: &str| -> String {
                            s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
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
                }
            }
        }
    }

    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list
}

#[tauri::command(async)]
fn rename_instance(instance_path: String, new_name: String) -> Result<String, String> {
    let path = std::path::Path::new(&instance_path);
    if !path.exists() {
        return Err("La ruta de la instancia no existe.".to_string());
    }

    let cfg_file = path.join("instance.cfg");
    if cfg_file.exists() {
        let content = std::fs::read_to_string(&cfg_file).map_err(|e| e.to_string())?;
        let mut new_lines = Vec::new();
        let mut found = false;
        for line in content.lines() {
            if line.trim().starts_with("name=") {
                new_lines.push(format!("name={}", new_name.trim()));
                found = true;
            } else {
                new_lines.push(line.to_string());
            }
        }
        if !found {
            new_lines.push(format!("name={}", new_name.trim()));
        }
        std::fs::write(&cfg_file, new_lines.join("\n")).map_err(|e| e.to_string())?;
    }

    let battly_file = path.join("instance.json");
    if battly_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&battly_file) {
            if let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(obj) = json.as_object_mut() {
                    obj.insert("name".to_string(), serde_json::Value::String(new_name.trim().to_string()));
                    let _ = std::fs::write(&battly_file, serde_json::to_string_pretty(&json).unwrap_or_default());
                }
            }
        }
    }

    Ok(new_name)
}

/// The category commands run off the UI thread and may overlap: serialize the
/// load-modify-write of the categories file so one save can't drop the other.
static USER_CATEGORIES_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tauri::command(async)]
fn set_mod_user_category(mod_keys: Vec<String>, category: String) -> Result<(), String> {
    let _lock = USER_CATEGORIES_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut map = scanner::load_user_categories();
    let norm_category = knowledge::canonical_category(&category).to_string();
    for k in mod_keys {
        let trimmed = k.trim().to_lowercase();
        if !scanner::is_generic_key(&trimmed) {
            map.insert(trimmed, norm_category.clone());
        }
    }
    // Clean up any accidentally stored generic keys
    for gk in scanner::GENERIC_KEYS {
        map.remove(*gk);
    }
    let path = scanner::get_user_categories_path();
    let json = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command(async)]
fn reset_mod_user_category(mod_keys: Vec<String>) -> Result<(), String> {
    let _lock = USER_CATEGORIES_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut map = scanner::load_user_categories();
    for k in mod_keys {
        let trimmed = k.trim().to_lowercase();
        if !trimmed.is_empty() {
            map.remove(&trimmed);
        }
    }
    let path = scanner::get_user_categories_path();
    let json = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command(async)]
fn get_user_categories() -> std::collections::HashMap<String, String> {
    scanner::load_user_categories()
}

#[tauri::command(async)]
fn infer_mod_default_category(
    name: String,
    slug: String,
    raw_desc: String,
) -> String {
    if let Some(km) = knowledge::find_known_mod(&slug, &slug) {
        return knowledge::canonical_category(km.category).to_string();
    }
    let (cat, _, _) = knowledge::infer_category_and_tags(&name, &raw_desc, &slug);
    knowledge::canonical_category(cat).to_string()
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct MotionBgWallpaper {
    pub title: String,
    pub page_slug: String,
    pub media_id: String,
    pub file_slug: String,
    pub thumb_url: String,
    pub video_url: String,
    pub video_4k_url: String,
    pub video_hd_url: String,
    pub preview_url: String,
}

fn create_silent_command(program: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// One page of a MotionBGs catalogue.
#[derive(serde::Serialize, Clone, Debug)]
pub struct MotionBgPage {
    pub items: Vec<MotionBgWallpaper>,
    /// The site links a following page.
    pub has_next: bool,
    /// The tag the following pages hang from: the one asked for, or the one the site answered
    /// a search with. Empty when there are no more pages.
    pub tag: String,
}

const MOTIONBGS_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)";

/// Only what a tag of the site is made of (it goes into a URL and into a file name).
fn motionbgs_tag(tag: &str) -> String {
    tag.trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

fn motionbgs_cache_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("modcomparer_mbg_cache");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Whether the file was written less than 12 hours ago.
fn motionbgs_cache_is_fresh(file: &std::path::Path) -> bool {
    std::fs::metadata(file)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|m| m.elapsed().ok())
        .map(|e| e.as_secs() < 12 * 3600)
        .unwrap_or(false)
}

/// The HTML of a page of the site, from the cache in the temp dir (valid for 12 hours, which
/// makes opening the catalogue instantaneous) or downloaded with curl.
fn motionbgs_html(url: &str, cache_file: &std::path::Path) -> Result<String, String> {
    let mut html = String::new();

    if motionbgs_cache_is_fresh(cache_file) {
        if let Ok(cached_content) = std::fs::read_to_string(cache_file) {
            if !cached_content.trim().is_empty() {
                html = cached_content;
            }
        }
    }

    if html.is_empty() {
        let output = create_silent_command("curl.exe")
            .args(["-s", "-L", "--compressed", "--max-time", "10", "-A", MOTIONBGS_AGENT, url])
            .output()
            .map_err(|e| format!("Error al ejecutar curl: {}", e))?;

        if output.status.success() {
            html = String::from_utf8_lossy(&output.stdout).to_string();
            // Write to cache
            let _ = std::fs::write(cache_file, &html);
        } else if cache_file.exists() {
            // Fallback to expired cache if network failed
            if let Ok(cached) = std::fs::read_to_string(cache_file) {
                html = cached;
            }
        }
    }

    if html.is_empty() {
        return Err("No se pudo conectar con motionbgs.com".to_string());
    }
    Ok(html)
}

fn motionbgs_parse(html: &str) -> Result<Vec<MotionBgWallpaper>, String> {
    let regex = regex::Regex::new(r#"(?is)<a\s+title="([^"]+)"\s+href=([^\s>]+)>[\s\S]*?<picture>[\s\S]*?/i/c/[^/]+/media/(\d+)/([^"\s>]+\.jpg)[\s\S]*?<span class=ttl>(.*?)</span>"#)
        .map_err(|e| e.to_string())?;

    let mut list = Vec::new();
    for cap in regex.captures_iter(html) {
        let raw_title = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        let page_slug = cap.get(2).map(|m| m.as_str()).unwrap_or("").trim().trim_start_matches('/');
        let media_id = cap.get(3).map(|m| m.as_str()).unwrap_or("").trim();
        let image_file_name = cap.get(4).map(|m| m.as_str()).unwrap_or("").trim();
        let clean_title = cap.get(5).map(|m| m.as_str()).unwrap_or("").trim();

        // Extract base slug for video file by removing resolutions & .jpg
        let mut file_slug = image_file_name;
        if file_slug.ends_with(".jpg") {
            file_slug = &file_slug[..file_slug.len() - 4];
        }
        for res in &[".3840x2160", ".1920x1080", ".2560x1440", ".1366x768", ".1280x720"] {
            if file_slug.ends_with(res) {
                file_slug = &file_slug[..file_slug.len() - res.len()];
                break;
            }
        }

        let title = if !clean_title.is_empty() {
            clean_title.to_string()
        } else {
            raw_title.to_string()
        };

        list.push(MotionBgWallpaper {
            title,
            page_slug: page_slug.to_string(),
            media_id: media_id.to_string(),
            file_slug: file_slug.to_string(),
            thumb_url: format!("https://motionbgs.com/i/c/546x308/media/{}/{}", media_id, image_file_name),
            video_url: format!("https://motionbgs.com/media/{}/{}.1920x1080.mp4", media_id, file_slug),
            video_4k_url: format!("https://motionbgs.com/media/{}/{}.3840x2160.mp4", media_id, file_slug),
            video_hd_url: format!("https://motionbgs.com/media/{}/{}.960x540.mp4", media_id, file_slug),
            preview_url: format!("https://motionbgs.com/media/{}/{}.960x540.mp4", media_id, file_slug),
        });
    }

    Ok(list)
}

#[tauri::command(async)]
fn fetch_motionbgs_wallpapers(query: String, tag: String) -> Result<Vec<MotionBgWallpaper>, String> {
    fetch_motionbgs_page(query, tag, 1).map(|page| page.items)
}

/// A page of a tag, or the results of a search. On the site only tags have pages
/// (`/tag:name/2/`): a search is a single page, unless the site answers it with a tag, and then
/// its following pages are asked for by that tag (`tag` of the answer).
#[tauri::command(async)]
fn fetch_motionbgs_page(query: String, tag: String, page: u32) -> Result<MotionBgPage, String> {
    let clean_query = query.trim().to_string();
    let clean_tag = match motionbgs_tag(&tag) {
        t if t.is_empty() => "minecraft".to_string(),
        t => t,
    };

    let url = if page > 1 {
        format!("https://motionbgs.com/tag:{}/{}/", clean_tag, page)
    } else if !clean_query.is_empty() {
        let q_encoded = clean_query.replace(' ', "+");
        format!("https://motionbgs.com/search?q={}", q_encoded)
    } else {
        format!("https://motionbgs.com/tag:{}/", clean_tag)
    };

    let query_in_name: String = clean_query
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
        .collect();
    let cache_file = motionbgs_cache_dir().join(format!(
        "modcomparer_mbg_{}_{}_{:x}.html",
        clean_tag,
        query_in_name,
        md5_digest(&url)
    ));

    let html = motionbgs_html(&url, &cache_file)?;
    let items = motionbgs_parse(&html)?;

    // <link href=https://motionbgs.com/tag:minecraft/2/ rel=next>
    let next = regex::Regex::new(r#"(?i)<link\s+href=["']?https://motionbgs\.com/tag:([^/"'\s>]+)/\d+/["']?\s+rel=["']?next"#)
        .map_err(|e| e.to_string())?;
    let next_tag = next.captures(&html).and_then(|cap| cap.get(1)).map(|m| motionbgs_tag(m.as_str()));
    let has_next = next_tag.is_some() && !items.is_empty();
    let tag = match next_tag {
        Some(t) if has_next => t,
        _ if page > 1 || clean_query.is_empty() => clean_tag,
        _ => String::new(),
    };

    Ok(MotionBgPage { items, has_next, tag })
}

/// How many pages a tag has. The site does not say, but a page past the last one answers 404:
/// the last one is looked for by doubling and then halving (a dozen header-only requests for a
/// tag of a hundred pages). Remembered for 12 hours.
#[tauri::command(async)]
fn count_motionbgs_pages(tag: String) -> Result<u32, String> {
    let clean_tag = motionbgs_tag(&tag);
    if clean_tag.is_empty() {
        return Err("Etiqueta vacía".to_string());
    }

    let cache_file = motionbgs_cache_dir().join(format!("modcomparer_mbg_{}_pages.txt", clean_tag));
    if motionbgs_cache_is_fresh(&cache_file) {
        if let Some(pages) = std::fs::read_to_string(&cache_file).ok().and_then(|s| s.trim().parse::<u32>().ok()) {
            if pages > 0 {
                return Ok(pages);
            }
        }
    }

    let exists = |page: u32| -> Result<bool, String> {
        let url = format!("https://motionbgs.com/tag:{}/{}/", clean_tag, page);
        let output = create_silent_command("curl.exe")
            .args(["-s", "-I", "-L", "--max-time", "8", "-o", "NUL", "-w", "%{http_code}", "-A", MOTIONBGS_AGENT, &url])
            .output()
            .map_err(|e| format!("Error al ejecutar curl: {}", e))?;
        match String::from_utf8_lossy(&output.stdout).trim() {
            "200" => Ok(true),
            "404" => Ok(false),
            other => Err(format!("motionbgs.com respondió {}", other)),
        }
    };

    if !exists(1)? {
        return Ok(0);
    }
    // `low` exists; `high` is the first one found that does not
    let mut low = 1u32;
    let mut high = 2u32;
    while exists(high)? {
        low = high;
        if high >= 4096 {
            break;
        }
        high *= 2;
    }
    while high > low + 1 {
        let middle = low + (high - low) / 2;
        if exists(middle)? {
            low = middle;
        } else {
            high = middle;
        }
    }

    let _ = std::fs::write(&cache_file, low.to_string());
    Ok(low)
}

fn md5_digest(s: &str) -> u64 {
    // Simple fast 64-bit hash for cache filenames
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[tauri::command]
async fn download_wallpaper_file(url: String, filename: String, thumb_url: Option<String>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let userprofile = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\Default".to_string());
        let downloads_dir = std::path::PathBuf::from(userprofile).join("Downloads").join("ModComparer_Wallpapers");

        if !downloads_dir.exists() {
            let _ = std::fs::create_dir_all(&downloads_dir);
        }

        let clean_filename = filename
            .chars()
            .map(|c| if ['/', '\\', ':', '*', '?', '"', '<', '>', '|'].contains(&c) { '_' } else { c })
            .collect::<String>();

        let dest = downloads_dir.join(&clean_filename);

        // Also download thumbnail image if provided
        if let Some(t_url) = thumb_url {
            if !t_url.trim().is_empty() {
                let mut base_name = clean_filename.replace(".mp4", "").replace(".webm", "");
                if base_name.ends_with("_4k") {
                    base_name = base_name.trim_end_matches("_4k").to_string();
                } else if base_name.ends_with("_1080p") {
                    base_name = base_name.trim_end_matches("_1080p").to_string();
                }
                let thumb_dest = downloads_dir.join(format!("{}.jpg", base_name));
                if !thumb_dest.exists() {
                    let _ = create_silent_command("curl.exe")
                        .args([
                            "-L",
                            "-s",
                            "--max-time", "15",
                            "-o",
                            thumb_dest.to_str().unwrap_or("thumb.jpg"),
                            &t_url,
                        ])
                        .output();
                }
            }
        }

        // If file already exists and is valid size (>100KB), return immediately!
        if dest.exists() {
            if let Ok(meta) = std::fs::metadata(&dest) {
                if meta.len() > 100_000 {
                    return Ok(dest.to_string_lossy().to_string());
                }
            }
        }

        let output = create_silent_command("curl.exe")
            .args([
                "-L",
                "-s",
                "--compressed",
                "--max-time", "180",
                "-o",
                dest.to_str().unwrap_or("wallpaper.mp4"),
                &url,
            ])
            .output()
            .map_err(|e| format!("Error al ejecutar curl: {}", e))?;

        if !output.status.success() {
            return Err("Falló la descarga del archivo de video.".to_string());
        }

        Ok(dest.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(async)]
fn check_wallpaper_file(filename: String) -> Option<String> {
    let userprofile = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\Default".to_string());
    let downloads_dir = std::path::PathBuf::from(userprofile).join("Downloads").join("ModComparer_Wallpapers");
    let clean_filename = filename
        .chars()
        .map(|c| if ['/', '\\', ':', '*', '?', '"', '<', '>', '|'].contains(&c) { '_' } else { c })
        .collect::<String>();

    let dest = downloads_dir.join(&clean_filename);
    if dest.exists() {
        if let Ok(meta) = std::fs::metadata(&dest) {
            if meta.len() > 100_000 {
                return Some(dest.to_string_lossy().to_string());
            }
        }
    }
    None
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SteamWallpaperInfo {
    pub id: String,
    pub title: String,
    pub video_path: String,
    pub preview_path: String,
    pub preview_data_url: Option<String>,
    pub tags: Vec<String>,
    pub file_size_mb: f64,
    pub is_scene: bool,
    pub project_path: Option<String>,
    pub properties: Option<serde_json::Value>,
}

fn generate_animated_preview_for_folder(folder: &std::path::Path) -> Option<std::path::PathBuf> {
    let mp4_path = folder.join("preview.mp4");
    if mp4_path.exists() {
        return Some(mp4_path);
    }
    let gif_path = folder.join("preview.gif");
    if gif_path.exists() {
        let _ = create_silent_command("ffmpeg")
            .args([
                "-y",
                "-i",
                gif_path.to_str().unwrap_or("preview.gif"),
                "-movflags",
                "faststart",
                "-pix_fmt",
                "yuv420p",
                mp4_path.to_str().unwrap_or("preview.mp4"),
            ])
            .output();
        if mp4_path.exists() {
            return Some(mp4_path);
        }
    }

    // Check if any native mp4 or webm is in the folder
    if let Ok(entries) = std::fs::read_dir(folder) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if ext.eq_ignore_ascii_case("mp4") || ext.eq_ignore_ascii_case("webm") {
                    return Some(p);
                }
            }
        }
    }

    None
}

fn parse_steam_wallpaper_folder(folder: &std::path::Path) -> Option<SteamWallpaperInfo> {
    if !folder.is_dir() {
        return None;
    }
    let project_json = folder.join("project.json");
    let (title, video_rel, preview_rel, tags, is_scene_meta, properties) = if let Ok(content) = std::fs::read_to_string(&project_json) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
            let t = val.get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Fondo Wallpaper Engine")
                .trim()
                .to_string();
            let vf = val.get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let pf = val.get("preview")
                .and_then(|v| v.as_str())
                .unwrap_or("preview.jpg")
                .to_string();
            let tg = val.get("tags")
                .and_then(|t| t.as_array())
                .map(|arr| {
                    arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()
                })
                .unwrap_or_default();
            let typ = val.get("type").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
            let scn = typ == "scene" || typ == "web" || vf.to_lowercase().ends_with(".json") || vf.to_lowercase().ends_with(".pkg") || vf.to_lowercase().ends_with(".html");
            let props = val.get("general").and_then(|g| g.get("properties")).cloned();
            (t, vf, pf, tg, scn, props)
        } else {
            (folder.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), String::new(), "preview.jpg".to_string(), vec![], false, None)
        }
    } else {
        (folder.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), String::new(), "preview.jpg".to_string(), vec![], false, None)
    };

    let is_scene = is_scene_meta
        || folder.join("scene.pkg").exists()
        || folder.join("scene.json").exists()
        || folder.join("index.html").exists()
        || folder.join("shaders").exists();

    let project_path = if project_json.exists() {
        Some(project_json.to_string_lossy().to_string())
    } else {
        None
    };

    let video_file = folder.join(&video_rel);
    let preview_file = folder.join(&preview_rel);

    let workshop_id = folder.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let is_video = video_rel.to_lowercase().ends_with(".mp4")
        || video_rel.to_lowercase().ends_with(".webm");

    let actual_video_file = if is_scene {
        let pkg_path = folder.join("scene.pkg");
        if pkg_path.exists() {
            pkg_path
        } else if folder.join("project.json").exists() {
            folder.join("project.json")
        } else if folder.join("index.html").exists() {
            folder.join("index.html")
        } else {
            let mut found = None;
            if let Ok(entries) = std::fs::read_dir(folder) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                        if ext.eq_ignore_ascii_case("mp4") || ext.eq_ignore_ascii_case("webm") {
                            found = Some(p);
                            break;
                        }
                    }
                }
            }
            found.unwrap_or_else(|| {
                if preview_file.exists() {
                    preview_file.clone()
                } else if folder.join("preview.jpg").exists() {
                    folder.join("preview.jpg")
                } else {
                    folder.join("scene.pkg")
                }
            })
        }
    } else if !video_rel.is_empty() && video_file.exists() && is_video {
        video_file
    } else {
        let mut found = None;
        if let Ok(entries) = std::fs::read_dir(folder) {
            for entry in entries.flatten() {
                let p = entry.path();
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    if ext.eq_ignore_ascii_case("mp4") || ext.eq_ignore_ascii_case("webm") {
                        found = Some(p);
                        break;
                    }
                }
            }
        }
        if found.is_none() {
            let mp4_path = folder.join("preview.mp4");
            if mp4_path.exists() {
                found = Some(mp4_path);
            }
        }
        found.unwrap_or_else(|| {
            if preview_file.exists() {
                preview_file.clone()
            } else if folder.join("preview.jpg").exists() {
                folder.join("preview.jpg")
            } else {
                folder.join("preview.mp4")
            }
        })
    };

    let size_mb = if is_scene && actual_video_file.extension().and_then(|e| e.to_str()).map(|ext| ext.eq_ignore_ascii_case("json") || ext.eq_ignore_ascii_case("html")).unwrap_or(false) {
        let total_bytes: u64 = walkdir::WalkDir::new(folder)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum();
        (total_bytes as f64) / 1_048_576.0
    } else {
        std::fs::metadata(&actual_video_file)
            .map(|m| (m.len() as f64) / 1_048_576.0)
            .unwrap_or(0.0)
    };

    let preview_path = if preview_file.exists() {
        preview_file.to_string_lossy().to_string()
    } else if folder.join("preview.jpg").exists() {
        folder.join("preview.jpg").to_string_lossy().to_string()
    } else if folder.join("preview.png").exists() {
        folder.join("preview.png").to_string_lossy().to_string()
    } else if folder.join("preview.gif").exists() {
        folder.join("preview.gif").to_string_lossy().to_string()
    } else {
        let mut any_img_str = String::new();
        // 1. Check all other candidate directories for this workshop ID
        if !workshop_id.is_empty() {
            for base in get_steam_candidate_dirs() {
                let alt_folder = base.join(&workshop_id);
                if alt_folder.exists() {
                    for img_name in ["preview.jpg", "preview.png", "preview.gif"] {
                        let alt_img = alt_folder.join(img_name);
                        if alt_img.exists() {
                            let dest = folder.join(img_name);
                            let _ = std::fs::copy(&alt_img, &dest);
                            any_img_str = dest.to_string_lossy().to_string();
                            break;
                        }
                    }
                }
                if !any_img_str.is_empty() {
                    break;
                }
            }
        }

        // 2. Scan folder for any image extension
        if any_img_str.is_empty() {
            if let Ok(entries) = std::fs::read_dir(folder) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                        if ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") || ext.eq_ignore_ascii_case("png") || ext.eq_ignore_ascii_case("gif") {
                            any_img_str = p.to_string_lossy().to_string();
                            break;
                        }
                    }
                }
            }
        }

        // 3. Fallback: Query Steam API details and download preview thumbnail
        if any_img_str.is_empty() && !workshop_id.is_empty() && workshop_id.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(details) = fetch_steam_item_details(workshop_id.clone()) {
                if !details.preview_url.is_empty() && details.preview_url.starts_with("http") {
                    let dest = folder.join("preview.jpg");
                    let _ = create_silent_command("curl.exe")
                        .args(["-s", "-L", "--max-time", "6", "-o", &dest.to_string_lossy(), &details.preview_url])
                        .output();
                    if dest.exists() {
                        any_img_str = dest.to_string_lossy().to_string();
                    }
                }
            }
        }

        any_img_str
    };

    Some(SteamWallpaperInfo {
        id: workshop_id,
        title,
        video_path: actual_video_file.to_string_lossy().to_string(),
        preview_path,
        preview_data_url: None,
        tags,
        file_size_mb: (size_mb * 10.0).round() / 10.0,
        is_scene,
        project_path,
        properties,
    })
}

fn get_steam_candidate_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = vec![
        std::path::PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\workshop\content\431960"),
        std::path::PathBuf::from(r"C:\Program Files\Steam\steamapps\workshop\content\431960"),
        std::path::PathBuf::from(r"D:\SteamLibrary\steamapps\workshop\content\431960"),
        std::path::PathBuf::from(r"E:\SteamLibrary\steamapps\workshop\content\431960"),
        std::path::PathBuf::from(r"C:\dev\Comparer\tools\steamcmd\steamapps\workshop\content\431960"),
        std::path::PathBuf::from(r"C:\dev\Comparer\tools\depotdownloader\downloads\431960"),
    ];
    if let Ok(cur) = std::env::current_dir() {
        dirs.push(cur.join("tools").join("steamcmd").join("steamapps").join("workshop").join("content").join("431960"));
        dirs.push(cur.join("tools").join("depotdownloader").join("downloads").join("431960"));
    }
    dirs
}

fn scan_steam_workshop_wallpapers_inner() -> Vec<SteamWallpaperInfo> {
    let mut results: Vec<SteamWallpaperInfo> = Vec::new();

    for base in get_steam_candidate_dirs() {
        if base.exists() {
            if let Ok(entries) = std::fs::read_dir(&base) {
                for entry in entries.flatten() {
                    let folder = entry.path();
                    if folder.is_dir() {
                        if let Some(info) = parse_steam_wallpaper_folder(&folder) {
                            if let Some(existing) = results.iter_mut().find(|r| r.id == info.id) {
                                if existing.preview_path.is_empty() && !info.preview_path.is_empty() {
                                    *existing = info;
                                }
                            } else {
                                results.push(info);
                            }
                        }
                    }
                }
            }
        }
    }

    results.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    results
}

#[tauri::command]
async fn get_steam_workshop_wallpapers() -> Vec<SteamWallpaperInfo> {
    tauri::async_runtime::spawn_blocking(scan_steam_workshop_wallpapers_inner)
        .await
        .unwrap_or_default()
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SteamOnlineItem {
    pub id: String,
    pub title: String,
    pub preview_url: String,
    pub workshop_url: String,
    pub is_installed: bool,
    pub local_video_path: Option<String>,
    pub is_scene: bool,
    pub project_path: Option<String>,
}

#[tauri::command(async)]
fn fetch_steam_item_details(id: String) -> Result<SteamOnlineItem, String> {
    let clean_id = id.chars().filter(|c| c.is_ascii_digit()).collect::<String>();
    if clean_id.is_empty() {
        return Err("ID de Steam Workshop no válido.".to_string());
    }

    let url = "https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/";
    let post_body = format!("itemcount=1&publishedfileids[0]={}", clean_id);

    let output = create_silent_command("curl.exe")
        .args([
            "-s",
            "-L",
            "--compressed",
            "--max-time", "10",
            "-X", "POST",
            url,
            "-d", &post_body,
        ])
        .output()
        .map_err(|e| format!("Error al consultar API de Steam: {}", e))?;

    if !output.status.success() {
        return Err("No se pudo conectar con el servicio de Steam.".to_string());
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| format!("Respuesta no válida de Steam: {}", e))?;

    let details = val.get("response")
        .and_then(|r| r.get("publishedfiledetails"))
        .and_then(|arr| arr.get(0))
        .ok_or_else(|| "No se encontró el item en Steam Workshop.".to_string())?;

    let result_code = details.get("result").and_then(|r| r.as_i64()).unwrap_or(0);
    if result_code != 1 {
        return Err("El fondo no existe o es privado en Steam Workshop.".to_string());
    }

    let title = details.get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("Fondo de Steam Workshop")
        .trim()
        .to_string();

    let preview_url = details.get("preview_url")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string();

    let workshop_url = format!("https://steamcommunity.com/sharedfiles/filedetails/?id={}", clean_id);

    let mut is_installed = false;
    let mut local_video_path = None;
    let mut is_scene = false;
    let mut project_path = None;

    for base in get_steam_candidate_dirs() {
        let item_folder = base.join(&clean_id);
        if item_folder.exists() {
            if let Some(info) = parse_steam_wallpaper_folder(&item_folder) {
                is_installed = true;
                local_video_path = Some(info.video_path);
                is_scene = info.is_scene;
                project_path = info.project_path;
                break;
            }
        }
    }

    Ok(SteamOnlineItem {
        id: clean_id,
        title,
        preview_url,
        workshop_url,
        is_installed,
        local_video_path,
        is_scene,
        project_path,
    })
}

#[tauri::command]
async fn download_steam_workshop_item(app: tauri::AppHandle, id: String) -> Result<SteamWallpaperInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let clean_id = id.chars().filter(|c| c.is_ascii_digit()).collect::<String>();
        if clean_id.is_empty() {
            return Err("ID de Steam Workshop no válido.".to_string());
        }

        let _guard = STEAM_DOWNLOAD_LOCK.lock().unwrap();

        let _ = app.emit("steam-download-progress", serde_json::json!({
            "id": &clean_id,
            "percent": 5,
            "status": "Comprobando archivos..."
        }));

        // 1. Check if already installed
        for base in get_steam_candidate_dirs() {
            let item_folder = base.join(&clean_id);
            if item_folder.exists() {
                if let Some(info) = parse_steam_wallpaper_folder(&item_folder) {
                    let _ = app.emit("steam-download-progress", serde_json::json!({
                        "id": &clean_id,
                        "percent": 100,
                        "status": "✓ Ya instalado"
                    }));
                    return Ok(info);
                }
            }
        }

        // 2. Target folder for DepotDownloaderMod
        let target_base = std::path::PathBuf::from(r"C:\dev\Comparer\tools\depotdownloader\downloads\431960");
        let target_folder = target_base.join(&clean_id);
        let _ = std::fs::create_dir_all(&target_folder);

        // 3. Find DepotDownloaderMod.exe
        let depot_paths = [
            std::path::PathBuf::from(r"C:\dev\Comparer\tools\depotdownloader\DepotDownloaderMod.exe"),
            std::env::current_dir().unwrap_or_default().join("tools").join("depotdownloader").join("DepotDownloaderMod.exe"),
            std::path::PathBuf::from(r"tools\depotdownloader\DepotDownloaderMod.exe"),
        ];

        let depot_exe = depot_paths.iter().find(|p| p.exists())
            .ok_or_else(|| "No se encontró DepotDownloaderMod.exe".to_string())?;

        let work_dir = depot_exe.parent().unwrap_or_else(|| std::path::Path::new("."));

        // Clean up any stale/orphaned DepotDownloader processes
        let _ = create_silent_command("taskkill")
            .args(["/F", "/IM", "DepotDownloaderMod.exe"])
            .output();

        let mut last_err = String::new();
        let pct_re = regex::Regex::new(r"^\s*(\d{1,3}(?:\.\d{1,2})?)%\s+(.*)$").unwrap();

        // Retry loop: up to 3 attempts with account adgjl1182
        for attempt in 1..=3 {
            let _ = app.emit("steam-download-progress", serde_json::json!({
                "id": &clean_id,
                "percent": 10,
                "status": if attempt > 1 { format!("Reintentando conexión ({}/3)...", attempt) } else { "Conectando a Steam...".to_string() }
            }));

            let mut cmd = create_silent_command(depot_exe.to_str().unwrap_or("DepotDownloaderMod.exe"));
            cmd.current_dir(work_dir)
                .args([
                    "-app", "431960",
                    "-pubfile", &clean_id,
                    "-username", "adgjl1182",
                    "-password", "QETUO99999",
                    "-dir", target_folder.to_str().unwrap_or(""),
                    "-verify-all",
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());

            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    last_err = e.to_string();
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    continue;
                }
            };

            // Read stderr in background thread
            let stderr_thread = {
                let stderr = child.stderr.take();
                std::thread::spawn(move || {
                    let mut s = String::new();
                    if let Some(pipe) = stderr {
                        let reader = BufReader::new(pipe);
                        for l in reader.lines().flatten() {
                            s.push_str(&l);
                            s.push('\n');
                        }
                    }
                    s
                })
            };

            // Read stdout in real time and emit live progress
            if let Some(stdout) = child.stdout.take() {
                let reader = BufReader::new(stdout);
                for line in reader.lines().flatten() {
                    let trim = line.trim();
                    if trim.is_empty() { continue; }

                    if trim.contains("Connecting to Steam") {
                        let _ = app.emit("steam-download-progress", serde_json::json!({
                            "id": &clean_id,
                            "percent": 15,
                            "status": "Conectando a Steam..."
                        }));
                    } else if trim.contains("Logging '") {
                        let _ = app.emit("steam-download-progress", serde_json::json!({
                            "id": &clean_id,
                            "percent": 25,
                            "status": "Autenticando..."
                        }));
                    } else if trim.contains("Got depot key") {
                        let _ = app.emit("steam-download-progress", serde_json::json!({
                            "id": &clean_id,
                            "percent": 35,
                            "status": "Licencia obtenida..."
                        }));
                    } else if trim.contains("Downloading depot") || trim.contains("Pre-allocating") {
                        let _ = app.emit("steam-download-progress", serde_json::json!({
                            "id": &clean_id,
                            "percent": 40,
                            "status": "Preparando descarga..."
                        }));
                    } else if let Some(caps) = pct_re.captures(trim) {
                        let file_pct: f32 = caps.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0.0);
                        let file_name = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                        let short_name = std::path::Path::new(file_name)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(file_name);
                        let overall = (40.0 + (file_pct * 0.55)).min(95.0) as u32;
                        let _ = app.emit("steam-download-progress", serde_json::json!({
                            "id": &clean_id,
                            "percent": overall,
                            "status": format!("{:.0}% {}", file_pct, short_name)
                        }));
                    }
                }
            }

            let _ = child.wait();
            let err_str = stderr_thread.join().unwrap_or_default();
            if !err_str.is_empty() {
                last_err = err_str;
            }

            // Post-process downloaded files in target_folder
            if target_folder.exists() {
                let mp4_path = target_folder.join("preview.mp4");

                let mut has_video = false;
                if let Ok(entries) = std::fs::read_dir(&target_folder) {
                    for entry in entries.flatten() {
                        if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                            if ext.eq_ignore_ascii_case("mp4") || ext.eq_ignore_ascii_case("webm") {
                                has_video = true;
                                break;
                            }
                        }
                    }
                }

                if !has_video && !mp4_path.exists() {
                    let _ = app.emit("steam-download-progress", serde_json::json!({
                        "id": &clean_id,
                        "percent": 96,
                        "status": "Generando animación y optimizando fondo..."
                    }));
                    let _ = generate_animated_preview_for_folder(&target_folder);
                }

                let proj_path = target_folder.join("project.json");
                if !proj_path.exists() {
                    let title = fetch_steam_item_details(clean_id.clone())
                        .map(|d| d.title)
                        .unwrap_or_else(|_| format!("Steam Wallpaper {}", clean_id));
                    let minimal = serde_json::json!({
                        "title": title,
                        "file": "preview.mp4",
                        "preview": "preview.jpg"
                    });
                    let _ = std::fs::write(&proj_path, minimal.to_string());
                }

                if let Some(info) = parse_steam_wallpaper_folder(&target_folder) {
                    let _ = app.emit("steam-download-progress", serde_json::json!({
                        "id": &clean_id,
                        "percent": 100,
                        "status": "✓ Descarga completa"
                    }));
                    return Ok(info);
                }
            }

            if attempt < 3 {
                std::thread::sleep(std::time::Duration::from_millis(2000));
            }
        }

        // Final check across all candidate dirs
        for base in get_steam_candidate_dirs() {
            let item_folder = base.join(&clean_id);
            if item_folder.exists() {
                if let Some(info) = parse_steam_wallpaper_folder(&item_folder) {
                    let _ = app.emit("steam-download-progress", serde_json::json!({
                        "id": &clean_id,
                        "percent": 100,
                        "status": "✓ Descarga completa"
                    }));
                    return Ok(info);
                }
            }
        }

        Err(if !last_err.is_empty() {
            format!("Error al descargar: {}", last_err.lines().next().unwrap_or("Fallo de conexión"))
        } else {
            format!("No se pudo descargar el item {}.", clean_id)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(async)]
fn sync_wallpaper_engine_desktop(_video_path: String) -> Result<(), String> {
    // Desactivado: El usuario no desea sincronizar ni cambiar el fondo de Wallpaper Engine
    Ok(())
}

#[tauri::command(async)]
fn launch_steamcmd_login() -> Result<String, String> {
    let steamcmd_dir = std::path::PathBuf::from(r"C:\dev\Comparer\tools\steamcmd");
    let helper_bat = steamcmd_dir.join("login_helper.bat");
    let exe_path = steamcmd_dir.join("steamcmd.exe");

    if !exe_path.exists() {
        return Err("No se encontró tools/steamcmd/steamcmd.exe".to_string());
    }

    let username = "REMOVED_STEAM_USER";
    let cmd = if helper_bat.exists() {
        format!("start \"SteamCMD Login - ModComparer\" cmd /c \"\"{}\" {}\"", helper_bat.display(), username)
    } else {
        format!("start \"SteamCMD Login - ModComparer\" \"{}\" +login {}", exe_path.display(), username)
    };

    let _ = std::process::Command::new("cmd.exe")
        .args(["/c", &cmd])
        .spawn()
        .map_err(|e| format!("Error al abrir consola de SteamCMD: {}", e))?;

    Ok("Consola de SteamCMD abierta.".to_string())
}

#[tauri::command(async)]
fn check_steam_item_installed(id: String) -> Option<SteamWallpaperInfo> {
    let clean_id = id.chars().filter(|c| c.is_ascii_digit()).collect::<String>();
    if clean_id.is_empty() {
        return None;
    }
    for base in get_steam_candidate_dirs() {
        let item_folder = base.join(&clean_id);
        if item_folder.exists() {
            if let Some(info) = parse_steam_wallpaper_folder(&item_folder) {
                return Some(info);
            }
        }
    }
    None
}

fn fetch_steam_workshop_online_inner(
    search: String,
    sort: String,
    tag: String,
    page: u32,
) -> Result<Vec<SteamOnlineItem>, String> {
    let clean_sort = match sort.as_str() {
        "mostrecent" => "mostrecent",
        "toprated" => "toprated",
        "totaluniquesubscribers" => "totaluniquesubscribers",
        _ => "trend",
    };

    let p = page.max(1);
    let mut url = format!(
        "https://steamcommunity.com/workshop/browse/?appid=431960&section=readytouseitems&browsesort={}&p={}",
        clean_sort, p
    );

    let clean_search = search.trim();
    if !clean_search.is_empty() {
        let encoded_search = clean_search.replace(' ', "+");
        url.push_str(&format!("&searchtext={}", encoded_search));
    }

    let clean_tag = tag.trim();
    if !clean_tag.is_empty() && clean_tag != "all" {
        let encoded_tag = clean_tag.replace(' ', "+");
        url.push_str(&format!("&requiredtags[]={}", encoded_tag));
    }

    let temp_dir = std::env::temp_dir().join("modcomparer_steam_cache");
    let _ = std::fs::create_dir_all(&temp_dir);
    let cache_file = temp_dir.join(format!("steam_{:x}.html", md5_digest(&url)));
    let mut html = String::new();
    let mut use_cache = false;

    if cache_file.exists() {
        if let Ok(metadata) = std::fs::metadata(&cache_file) {
            if let Some(elapsed) = metadata.modified().ok().and_then(|m| m.elapsed().ok()) {
                if elapsed.as_secs() < 300 {
                    if let Ok(cached_content) = std::fs::read_to_string(&cache_file) {
                        if !cached_content.trim().is_empty() {
                            html = cached_content;
                            use_cache = true;
                        }
                    }
                }
            }
        }
    }

    if !use_cache {
        let output = create_silent_command("curl.exe")
            .args([
                "-s",
                "-L",
                "--compressed",
                "--max-time",
                "15",
                "-A",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
                "-H",
                "Accept-Language: en-US,en;q=0.9,es;q=0.8",
                "-b",
                "birthtime=283993201; mature_content=1; wants_mature_content=1;",
                &url,
            ])
            .output()
            .map_err(|e| format!("Error al consultar Steam Workshop: {}", e))?;

        if output.status.success() {
            html = String::from_utf8_lossy(&output.stdout).to_string();
            let _ = std::fs::write(&cache_file, &html);
        } else if cache_file.exists() {
            if let Ok(cached) = std::fs::read_to_string(&cache_file) {
                html = cached;
            }
        }
    }

    if html.is_empty() {
        return Err("No se pudo conectar con Steam Community Workshop.".to_string());
    }

    // Check locally installed workshop items fast
    let candidate_dirs = get_steam_candidate_dirs();

    let mut results = Vec::new();
    let re = regex::Regex::new(r#"<a\s+href="https://steamcommunity\.com/sharedfiles/filedetails/\?id=(\d+)"[^>]*><img\s+src="([^"]+)"\s+alt="([^"]+)""#)
        .map_err(|e| e.to_string())?;

    for cap in re.captures_iter(&html) {
        let id = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
        let preview_raw = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let title_raw = cap.get(3).map(|m| m.as_str()).unwrap_or("");

        let preview_url = preview_raw.replace("&amp;", "&");
        let title = title_raw
            .replace("&amp;", "&")
            .replace("&#39;", "'")
            .replace("&quot;", "\"")
            .trim()
            .to_string();

        let workshop_url = format!("https://steamcommunity.com/sharedfiles/filedetails/?id={}", id);
        
        let mut is_installed = false;
        let mut local_video_path = None;
        let mut is_scene = false;
        let mut project_path = None;

        for base in &candidate_dirs {
            let item_folder = base.join(&id);
            if item_folder.is_dir() {
                if let Some(info) = parse_steam_wallpaper_folder(&item_folder) {
                    is_installed = true;
                    local_video_path = Some(info.video_path);
                    is_scene = info.is_scene;
                    project_path = info.project_path;
                    break;
                }
            }
        }

        results.push(SteamOnlineItem {
            id,
            title,
            preview_url,
            workshop_url,
            is_installed,
            local_video_path,
            is_scene,
            project_path,
        });
    }

    Ok(results)
}

#[tauri::command]
async fn fetch_steam_workshop_online(
    search: String,
    sort: String,
    tag: String,
    page: u32,
) -> Result<Vec<SteamOnlineItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        fetch_steam_workshop_online_inner(search, sort, tag, page)
    })
    .await
    .map_err(|e| format!("Error en tarea de Steam Workshop: {}", e))?
}

#[tauri::command(async)]
fn read_local_wallpaper_blob(video_path: String) -> Result<Vec<u8>, String> {
    let path = std::path::Path::new(&video_path);
    if !path.exists() {
        return Err("El archivo de video no existe.".to_string());
    }
    std::fs::read(path).map_err(|e| format!("Error al leer video: {}", e))
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DownloadedWallpaperItem {
    pub id: String,
    pub title: String,
    pub video_path: String,
    pub preview_path: Option<String>,
    pub preview_data_url: Option<String>,
    pub source: String, // "steam_workshop", "motionbgs", "imported"
    pub file_size_mb: f64,
    pub tags: Vec<String>,
    pub resolution: Option<String>,
    pub modified_time: u64,
    pub is_scene: bool,
    pub project_path: Option<String>,
    pub properties: Option<serde_json::Value>,
}

fn get_file_modified_time(path: &std::path::Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
        .unwrap_or(0)
}

fn get_all_downloaded_wallpapers_inner() -> Vec<DownloadedWallpaperItem> {
    let mut items: Vec<DownloadedWallpaperItem> = Vec::new();
    let mut seen_video_paths = std::collections::HashSet::new();

    // 1. Steam Workshop / Wallpaper Engine
    for steam_item in scan_steam_workshop_wallpapers_inner() {
        let v_path = std::path::PathBuf::from(&steam_item.video_path);
        let mtime = get_file_modified_time(&v_path);
        let preview = if !steam_item.preview_path.is_empty() {
            Some(steam_item.preview_path.clone())
        } else {
            None
        };
        seen_video_paths.insert(steam_item.video_path.to_lowercase());
        items.push(DownloadedWallpaperItem {
            id: steam_item.id,
            title: steam_item.title,
            video_path: steam_item.video_path,
            preview_path: preview,
            preview_data_url: steam_item.preview_data_url,
            source: "steam_workshop".to_string(),
            file_size_mb: steam_item.file_size_mb,
            tags: steam_item.tags,
            resolution: None,
            modified_time: mtime,
            is_scene: steam_item.is_scene,
            project_path: steam_item.project_path,
            properties: steam_item.properties,
        });
    }

    // 2. MotionBGs downloads (%USERPROFILE%\Downloads\ModComparer_Wallpapers)
    let userprofile = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\Default".to_string());
    let base_wallpapers_dir = std::path::PathBuf::from(userprofile).join("Downloads").join("ModComparer_Wallpapers");

    if base_wallpapers_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&base_wallpapers_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    if ext == "mp4" || ext == "webm" {
                        let path_str = p.to_string_lossy().to_string();
                        if seen_video_paths.insert(path_str.to_lowercase()) {
                            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("Fondo").to_string();
                            let mut res = None;
                            let mut clean_stem = stem.clone();
                            if clean_stem.ends_with("_4k") {
                                res = Some("4K".to_string());
                                clean_stem = clean_stem.trim_end_matches("_4k").to_string();
                            } else if clean_stem.ends_with("_1080p") {
                                res = Some("1080p".to_string());
                                clean_stem = clean_stem.trim_end_matches("_1080p").to_string();
                            }

                            // Convert kebab-case or snake_case to Title Case
                            let title = clean_stem
                                .replace(['-', '_'], " ")
                                .split_whitespace()
                                .map(|word| {
                                    let mut chars = word.chars();
                                    match chars.next() {
                                        None => String::new(),
                                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                                    }
                                })
                                .collect::<Vec<String>>()
                                .join(" ");

                            let size_mb = std::fs::metadata(&p)
                                .map(|m| (m.len() as f64) / 1_048_576.0)
                                .unwrap_or(0.0);
                            let mtime = get_file_modified_time(&p);

                            // Check preview image if exists in folder
                            let mut preview = None;
                            for img_ext in &["jpg", "jpeg", "png", "webp"] {
                                let test_img1 = base_wallpapers_dir.join(format!("{}.{}", clean_stem, img_ext));
                                let test_img2 = base_wallpapers_dir.join(format!("{}.{}", stem, img_ext));
                                if test_img1.exists() {
                                    preview = Some(test_img1.to_string_lossy().to_string());
                                    break;
                                } else if test_img2.exists() {
                                    preview = Some(test_img2.to_string_lossy().to_string());
                                    break;
                                }
                            }

                            items.push(DownloadedWallpaperItem {
                                id: stem.clone(),
                                title: if title.is_empty() { stem } else { title },
                                video_path: path_str,
                                preview_path: preview,
                                preview_data_url: None,
                                source: "motionbgs".to_string(),
                                file_size_mb: (size_mb * 10.0).round() / 10.0,
                                tags: vec!["MotionBGs".to_string()],
                                resolution: res,
                                modified_time: mtime,
                                is_scene: false,
                                project_path: None,
                                properties: None,
                            });
                        }
                    }
                }
            }
        }

        // 3. User Imported Wallpapers (%USERPROFILE%\Downloads\ModComparer_Wallpapers\imported)
        let imported_dir = base_wallpapers_dir.join("imported");
        if imported_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&imported_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() {
                        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                        if ["mp4", "webm", "mov", "mkv", "avi"].contains(&ext.as_str()) {
                            let path_str = p.to_string_lossy().to_string();
                            if seen_video_paths.insert(path_str.to_lowercase()) {
                                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("Video").to_string();
                                let size_mb = std::fs::metadata(&p)
                                    .map(|m| (m.len() as f64) / 1_048_576.0)
                                    .unwrap_or(0.0);
                                let mtime = get_file_modified_time(&p);

                                let mut imp_preview = None;
                                for img_ext in &["jpg", "jpeg", "png", "webp"] {
                                    let test_img = imported_dir.join(format!("{}.{}", stem, img_ext));
                                    if test_img.exists() {
                                        imp_preview = Some(test_img.to_string_lossy().to_string());
                                        break;
                                    }
                                }

                                items.push(DownloadedWallpaperItem {
                                    id: format!("imp_{}", stem),
                                    title: stem,
                                    video_path: path_str,
                                    preview_path: imp_preview,
                                    preview_data_url: None,
                                    source: "imported".to_string(),
                                    file_size_mb: (size_mb * 10.0).round() / 10.0,
                                    tags: vec!["Importado".to_string()],
                                    resolution: None,
                                    modified_time: mtime,
                                    is_scene: false,
                                    project_path: None,
                                    properties: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // Sort by modified time descending (newest first)
    items.sort_by(|a, b| b.modified_time.cmp(&a.modified_time));
    items
}

#[tauri::command]
async fn get_all_downloaded_wallpapers() -> Vec<DownloadedWallpaperItem> {
    tauri::async_runtime::spawn_blocking(get_all_downloaded_wallpapers_inner)
        .await
        .unwrap_or_default()
}

#[tauri::command]
fn pick_wallpaper_file() -> Option<String> {
    rfd::FileDialog::new()
        .set_title("Seleccionar video para fondo animado")
        .add_filter("Videos", &["mp4", "webm", "mov", "mkv", "avi"])
        .pick_file()
        .map(|p| p.to_string_lossy().to_string())
}

#[tauri::command(async)]
fn import_wallpaper_file(file_path: String) -> Result<DownloadedWallpaperItem, String> {
    let src = std::path::Path::new(&file_path);
    if !src.exists() || !src.is_file() {
        return Err("El archivo de video seleccionado no existe.".to_string());
    }

    let userprofile = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\Default".to_string());
    let imported_dir = std::path::PathBuf::from(userprofile)
        .join("Downloads")
        .join("ModComparer_Wallpapers")
        .join("imported");

    let _ = std::fs::create_dir_all(&imported_dir);

    let file_name = src.file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "Nombre de archivo no válido.".to_string())?;

    let dest = imported_dir.join(file_name);
    if src != dest {
        std::fs::copy(src, &dest).map_err(|e| format!("Error al copiar archivo: {}", e))?;
    }

    let stem = dest.file_stem().and_then(|s| s.to_str()).unwrap_or("Video").to_string();
    let size_mb = std::fs::metadata(&dest)
        .map(|m| (m.len() as f64) / 1_048_576.0)
        .unwrap_or(0.0);
    let mtime = get_file_modified_time(&dest);

    Ok(DownloadedWallpaperItem {
        id: format!("imp_{}", stem),
        title: stem,
        video_path: dest.to_string_lossy().to_string(),
        preview_path: None,
        preview_data_url: None,
        source: "imported".to_string(),
        file_size_mb: (size_mb * 10.0).round() / 10.0,
        tags: vec!["Importado".to_string()],
        resolution: None,
        modified_time: mtime,
        is_scene: false,
        project_path: None,
        properties: None,
    })
}

#[tauri::command(async)]
fn open_wallpapers_folder() -> Result<(), String> {
    let userprofile = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\Default".to_string());
    let folder = std::path::PathBuf::from(userprofile)
        .join("Downloads")
        .join("ModComparer_Wallpapers");
    let _ = std::fs::create_dir_all(&folder);
    open::that(&folder).map_err(|e| e.to_string())
}

fn force_remove_dir_all(dir: &std::path::Path) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
        let p = entry.path();
        if let Ok(metadata) = std::fs::metadata(p) {
            let mut perms = metadata.permissions();
            if perms.readonly() {
                perms.set_readonly(false);
                let _ = std::fs::set_permissions(p, perms);
            }
        }
    }
    let mut last_err = None;
    for _ in 0..5 {
        match std::fs::remove_dir_all(dir) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
    if let Some(e) = last_err {
        Err(e)
    } else {
        Ok(())
    }
}

#[tauri::command(async)]
fn delete_downloaded_wallpaper(video_path: String) -> Result<(), String> {
    let p = std::path::Path::new(&video_path);

    // Determine if this is a Steam Workshop item and find its ID
    let mut workshop_id_opt: Option<String> = None;

    // 1. Check path components for 431960/<id>
    let components: Vec<String> = p.iter().map(|c| c.to_string_lossy().to_string()).collect();
    for i in 0..components.len() {
        if components[i] == "431960" && i + 1 < components.len() {
            let next_comp = &components[i + 1];
            if next_comp.chars().all(|c| c.is_ascii_digit()) && !next_comp.is_empty() {
                workshop_id_opt = Some(next_comp.clone());
                break;
            }
        }
    }

    // 2. Check parent folder name if all digits
    if workshop_id_opt.is_none() {
        if let Some(parent) = p.parent() {
            if let Some(folder_name) = parent.file_name().and_then(|s| s.to_str()) {
                if folder_name.chars().all(|c| c.is_ascii_digit()) && !folder_name.is_empty() {
                    workshop_id_opt = Some(folder_name.to_string());
                }
            }
        }
    }

    // 3. Check if path itself is a numeric folder
    if workshop_id_opt.is_none() {
        if let Some(folder_name) = p.file_name().and_then(|s| s.to_str()) {
            if folder_name.chars().all(|c| c.is_ascii_digit()) && !folder_name.is_empty() {
                workshop_id_opt = Some(folder_name.to_string());
            }
        }
    }

    // Stop active wallpaper ONLY IF it is currently running in WE
    let cur_path = CURRENT_WE_PATH.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let is_currently_running_we = if cur_path.is_empty() {
        false
    } else {
        let cur_norm = cur_path.replace('\\', "/").to_lowercase();
        let del_norm = video_path.replace('\\', "/").to_lowercase();

        let same_path = cur_norm == del_norm;
        let same_wid = if let Some(ref wid) = workshop_id_opt {
            cur_norm.contains(&format!("/{}/", wid))
                || cur_norm.ends_with(&format!("/{}", wid))
                || cur_norm.contains(&format!("431960/{}", wid))
        } else {
            false
        };
        let inside_deleted_dir = if let Some(parent) = p.parent() {
            let parent_norm = parent.to_string_lossy().replace('\\', "/").to_lowercase();
            !parent_norm.is_empty() && (cur_norm.starts_with(&parent_norm) || cur_norm == parent_norm)
        } else {
            false
        };

        same_path || same_wid || inside_deleted_dir
    };

    if is_currently_running_we {
        let _ = stop_we_wallpaper();
        std::thread::sleep(std::time::Duration::from_millis(80));
    }

    let mut deleted_any = false;

    // If it's a Steam Workshop item, purge all instances across candidate directories and tell WE
    if let Some(ref wid) = workshop_id_opt {
        if let Some(we_exe) = find_wallpaper_engine_path() {
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x08000000;
                let _ = std::process::Command::new(&we_exe)
                    .args(["-control", "closeWallpaper", "-location", wid])
                    .creation_flags(CREATE_NO_WINDOW)
                    .status();
            }
        }

        // Delete from every candidate Steam workshop and download directory
        for base in get_steam_candidate_dirs() {
            let item_folder = base.join(wid);
            if item_folder.exists() {
                if force_remove_dir_all(&item_folder).is_ok() {
                    deleted_any = true;
                }
            }
        }

        // Also delete parent folder if it matches wid or contains scene.pkg / project.json
        if let Some(parent) = p.parent() {
            if parent.exists() && parent.is_dir() {
                let is_workshop_dir = parent.file_name().and_then(|s| s.to_str()) == Some(wid)
                    || parent.join("project.json").exists()
                    || parent.join("scene.pkg").exists();
                if is_workshop_dir {
                    if force_remove_dir_all(parent).is_ok() {
                        deleted_any = true;
                    }
                }
            }
        }
    }

    // If the path itself still exists (e.g. MotionBGs / user imported file or directory)
    if p.exists() {
        if p.is_dir() {
            force_remove_dir_all(p).map_err(|e| format!("Error al eliminar carpeta: {}", e))?;
            deleted_any = true;
        } else {
            // Delete associated preview images
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                if let Some(parent) = p.parent() {
                    for ext in &["jpg", "jpeg", "png", "webp", "gif"] {
                        let img = parent.join(format!("{}.{}", stem, ext));
                        if img.exists() {
                            let _ = std::fs::remove_file(img);
                        }
                        let clean_stem = stem.trim_end_matches("_4k").trim_end_matches("_1080p");
                        let img2 = parent.join(format!("{}.{}", clean_stem, ext));
                        if img2.exists() {
                            let _ = std::fs::remove_file(img2);
                        }
                    }
                }
            }
            std::fs::remove_file(p).map_err(|e| format!("Error al eliminar archivo: {}", e))?;
            deleted_any = true;
        }
    }

    if deleted_any {
        Ok(())
    } else if workshop_id_opt.is_some() {
        Ok(())
    } else {
        Err("El archivo o carpeta ya no existe en el disco.".to_string())
    }
}

/// While the launcher is minimized, tell WebView2 it is not visible (stops rendering,
/// lets Chromium throttle the page) and to keep its memory as low as possible
/// (drops caches, trims the GPU/renderer processes). Restored on un-minimize.
static WEBVIEW_BACKGROUND_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static WEBVIEW_FOCUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

fn set_webview_background_mode(window: &tauri::WebviewWindow, background: bool) {
    if WEBVIEW_BACKGROUND_MODE.swap(background, std::sync::atomic::Ordering::SeqCst) == background {
        return;
    }
    #[cfg(target_os = "windows")]
    {
        let _ = window.with_webview(move |wv| unsafe {
            let _ = wv.controller().SetIsVisible(!background);
        });
    }
    apply_webview_memory_level(window);
}

/// Focus changes only touch the memory target (the page keeps rendering, e.g. the video
/// background behind a game), minimizing additionally hides the WebView.
fn set_webview_focused(window: &tauri::WebviewWindow, focused: bool) {
    if WEBVIEW_FOCUSED.swap(focused, std::sync::atomic::Ordering::SeqCst) != focused {
        apply_webview_memory_level(window);
    }
}

/// LOW while the launcher is inactive (minimized or not focused): WebView2 drops caches
/// and trims its processes. NORMAL as soon as the user is back in the launcher.
fn apply_webview_memory_level(window: &tauri::WebviewWindow) {
    let low = WEBVIEW_BACKGROUND_MODE.load(std::sync::atomic::Ordering::SeqCst)
        || !WEBVIEW_FOCUSED.load(std::sync::atomic::Ordering::SeqCst);
    #[cfg(target_os = "windows")]
    {
        let _ = window.with_webview(move |wv| unsafe {
            use webview2_com::Microsoft::Web::WebView2::Win32::{
                ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
                COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
            };
            use windows_core::Interface;
            if let Ok(core) = wv.controller().CoreWebView2() {
                if let Ok(core19) = core.cast::<ICoreWebView2_19>() {
                    let level = if low {
                        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
                    } else {
                        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
                    };
                    let _ = core19.SetMemoryUsageTargetLevel(level);
                }
            }
        });
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (window, low);
}

/// The launcher must not behave like a browser tab: turn off WebView2's browser-only
/// shortcuts (F5 / Ctrl+R reload, Ctrl+F / F3 find, Ctrl+P print, F12 / Ctrl+Shift+I
/// DevTools, F7 caret browsing, Alt+Left/Right history...). Text editing keys (Ctrl+C/V/X,
/// Ctrl+A, Ctrl+Z, arrows) are not affected, and the page still receives every keydown, so
/// the launcher's own shortcuts keep working. Tauri has no option for this setting.
fn disable_browser_shortcuts(window: &tauri::WebviewWindow) {
    #[cfg(target_os = "windows")]
    {
        let _ = window.with_webview(|wv| unsafe {
            use webview2_com::Microsoft::Web::WebView2::Win32::{
                ICoreWebView2Settings3, ICoreWebView2Settings4,
            };
            use windows_core::Interface;
            if let Ok(settings) = wv.controller().CoreWebView2().and_then(|core| core.Settings()) {
                if let Ok(settings3) = settings.cast::<ICoreWebView2Settings3>() {
                    let _ = settings3.SetAreBrowserAcceleratorKeysEnabled(false);
                }
                // No "save password?" bubble or remembered-values dropdown on text fields.
                if let Ok(settings4) = settings.cast::<ICoreWebView2Settings4>() {
                    let _ = settings4.SetIsPasswordAutosaveEnabled(false);
                    let _ = settings4.SetIsGeneralAutofillEnabled(false);
                }
            }
        });
    }
    #[cfg(not(target_os = "windows"))]
    let _ = window;
}

#[tauri::command]
fn app_window_minimize(window: tauri::Window) {
    #[cfg(target_os = "windows")]
    {
        if let Ok(h) = window.hwnd() {
            refresh_taskbar_preview_cache(h.0 as we_win32::HWND);
        }
        if get_we_window_hwnd().is_some() {
            thaw_we_scene_before_window_ops();
        }
        unsafe {
            if let Some(hwnd_val) = get_we_window_hwnd() {
                we_win32::ShowWindow(hwnd_val as we_win32::HWND, we_win32::SW_HIDE);
            }
            if let Some(u_val) = get_underlay_hwnd() {
                we_win32::ShowWindow(u_val as we_win32::HWND, we_win32::SW_HIDE);
            }
        }
        *LAST_WE_GEOM.lock().unwrap_or_else(|e| e.into_inner()) = (0, 0, 0, 0);
    }
    let _ = window.minimize();
}

#[tauri::command]
fn app_window_toggle_maximize(window: tauri::Window) {
    if let Ok(is_max) = window.is_maximized() {
        if is_max {
            let _ = window.unmaximize();
        } else {
            let _ = window.maximize();
        }
    }
}

#[tauri::command]
fn app_window_close(window: tauri::Window) {
    save_window_state(&window);
    let _ = stop_we_wallpaper();
    window.app_handle().exit(0);
}

/// Size, position and maximized state of the launcher window, remembered between sessions in
/// ika_root/window-state.json. Taken from the window placement, which keeps the bounds of the
/// normal window also while it is maximized or minimized; when it is applied again Windows
/// moves the window back on screen if its monitor is gone.
#[cfg(target_os = "windows")]
mod window_state {
    use std::ffi::c_void;
    use std::sync::Mutex;

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct POINT {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    #[derive(Default)]
    #[allow(non_snake_case)]
    struct WINDOWPLACEMENT {
        length: u32,
        flags: u32,
        showCmd: u32,
        ptMinPosition: POINT,
        ptMaxPosition: POINT,
        rcNormalPosition: RECT,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetWindowPlacement(hwnd: *mut c_void, placement: *mut WINDOWPLACEMENT) -> i32;
        fn SetWindowPlacement(hwnd: *mut c_void, placement: *const WINDOWPLACEMENT) -> i32;
    }

    const SW_HIDE: u32 = 0;
    const SW_SHOWMINIMIZED: u32 = 2;
    const SW_SHOWMAXIMIZED: u32 = 3;
    const WPF_RESTORETOMAXIMIZED: u32 = 0x2;
    /// Smaller than this is not a state worth keeping (a broken file, a collapsed window).
    const MIN_SIZE: (i32, i32) = (400, 300);

    /// Last state written: it is saved on every focus loss, usually unchanged.
    static LAST_SAVED: Mutex<String> = Mutex::new(String::new());

    fn path() -> std::path::PathBuf {
        crate::mc::ika_root().join("window-state.json")
    }

    fn placement(hwnd: isize) -> Option<WINDOWPLACEMENT> {
        let mut wp = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
        (unsafe { GetWindowPlacement(hwnd as *mut c_void, &mut wp) } != 0).then_some(wp)
    }

    pub fn save(hwnd: isize) {
        let Some(wp) = placement(hwnd) else { return };
        let r = wp.rcNormalPosition;
        if r.right - r.left < MIN_SIZE.0 || r.bottom - r.top < MIN_SIZE.1 {
            return;
        }
        let maximized = wp.showCmd == SW_SHOWMAXIMIZED
            || (wp.showCmd == SW_SHOWMINIMIZED && wp.flags & WPF_RESTORETOMAXIMIZED != 0);
        let text = serde_json::json!({
            "left": r.left, "top": r.top, "right": r.right, "bottom": r.bottom, "maximized": maximized,
        })
        .to_string();
        let mut last = LAST_SAVED.lock().unwrap_or_else(|e| e.into_inner());
        if *last == text {
            return;
        }
        let _ = std::fs::create_dir_all(crate::mc::ika_root());
        if std::fs::write(path(), &text).is_ok() {
            *last = text;
        }
    }

    pub fn has_saved_state() -> bool {
        path().exists()
    }

    /// Applies the saved bounds to the window, which stays hidden. Returns whether it was
    /// maximized.
    pub fn restore(hwnd: isize) -> bool {
        let Ok(text) = std::fs::read_to_string(path()) else { return false };
        let Ok(state) = serde_json::from_str::<serde_json::Value>(&text) else { return false };
        let num = |key: &str| state.get(key).and_then(|v| v.as_i64()).map(|v| v as i32);
        let (Some(left), Some(top), Some(right), Some(bottom)) = (num("left"), num("top"), num("right"), num("bottom")) else {
            return false;
        };
        if right - left < MIN_SIZE.0 || bottom - top < MIN_SIZE.1 {
            return false;
        }
        let Some(mut wp) = placement(hwnd) else { return false };
        wp.flags = 0;
        wp.showCmd = SW_HIDE;
        wp.rcNormalPosition = RECT { left, top, right, bottom };
        unsafe {
            SetWindowPlacement(hwnd as *mut c_void, &wp);
        }
        state.get("maximized").and_then(|v| v.as_bool()).unwrap_or(false)
    }
}

/// What the user arranged in Inicio (floating windows, what is docked in the edge panels and how,
/// the welcome message, the clock) is kept in a file besides the WebView's storage: that storage
/// belongs to the page's origin, which `tauri dev` and a compiled launcher don't share, and it
/// goes away with the WebView data. The frontend owns the format (a JSON document).
fn home_layout_path() -> std::path::PathBuf {
    mc::ika_root().join("home-layout.json")
}

#[tauri::command(async)]
fn home_layout_load() -> Option<String> {
    std::fs::read_to_string(home_layout_path())
        .ok()
        .or_else(|| {
            let default_data = include_str!("../default-home-layout.json");
            let _ = home_layout_save(default_data.to_string());
            Some(default_data.to_string())
        })
}

#[tauri::command(async)]
fn home_layout_save(data: String) -> Result<(), String> {
    std::fs::create_dir_all(mc::ika_root()).map_err(|e| e.to_string())?;
    std::fs::write(home_layout_path(), data).map_err(|e| e.to_string())
}

fn save_window_state(window: &tauri::Window) {
    #[cfg(target_os = "windows")]
    if let Ok(h) = window.hwnd() {
        window_state::save(h.0 as isize);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = window;
}

#[tauri::command]
fn app_window_start_dragging(window: tauri::Window) {
    #[cfg(target_os = "windows")]
    {
        let app_hwnd = window.hwnd().map(|h| h.0 as we_win32::HWND).unwrap_or(std::ptr::null_mut());
        if !app_hwnd.is_null() {
            unsafe {
                we_win32::SetWindowPos(
                    app_hwnd,
                    0 as we_win32::HWND, // HWND_TOP
                    0, 0, 0, 0,
                    we_win32::SWP_NOMOVE | we_win32::SWP_NOSIZE | we_win32::SWP_NOACTIVATE,
                );
                we_win32::BringWindowToTop(app_hwnd);
            }
        }
    }
    let _ = window.start_dragging();
}

#[tauri::command]
fn app_window_show(window: tauri::Window) {
    let _ = window.show();
    let _ = window.set_focus();
}

#[cfg(target_os = "windows")]
#[allow(dead_code, non_camel_case_types)]
mod we_win32 {
    use std::ffi::c_void;

    pub type HWND = *mut c_void;
    pub type BOOL = i32;
    pub type LONG_PTR = isize;

    pub const GWL_STYLE: i32 = -16;
    pub const GWL_EXSTYLE: i32 = -20;
    pub const GWLP_HWNDPARENT: i32 = -8;
    pub const GWLP_WNDPROC: i32 = -4;

    pub const WS_POPUP: u32 = 0x80000000;
    pub const WS_CHILD: u32 = 0x40000000;
    pub const WS_VISIBLE: u32 = 0x10000000;
    pub const WS_CLIPSIBLINGS: u32 = 0x04000000;
    pub const WS_CAPTION: u32 = 0x00C00000;
    pub const WS_BORDER: u32 = 0x00800000;
    pub const WS_THICKFRAME: u32 = 0x00040000;

    pub const HWND_BOTTOM: HWND = 1 as HWND;
    pub const HWND_TOP: HWND = 0 as HWND;

    pub const WS_EX_TOOLWINDOW: u32 = 0x00000080;
    pub const WS_EX_APPWINDOW: u32 = 0x00040000;
    pub const WS_EX_NOACTIVATE: u32 = 0x08000000;
    pub const WS_EX_TRANSPARENT: u32 = 0x00000020;

    pub const SWP_NOSIZE: u32 = 0x0001;
    pub const SWP_NOMOVE: u32 = 0x0002;
    pub const SWP_NOZORDER: u32 = 0x0004;
    pub const SWP_NOACTIVATE: u32 = 0x0010;
    pub const SWP_FRAMECHANGED: u32 = 0x0020;
    pub const SWP_SHOWWINDOW: u32 = 0x0040;
    pub const SWP_NOCOPYBITS: u32 = 0x0100;
    pub const SW_HIDE: i32 = 0;
    pub const SW_SHOW: i32 = 5;
    pub const SW_SHOWNA: i32 = 8;
    pub const SW_SHOWNOACTIVATE: i32 = 4;

    #[repr(C)]
    pub struct ITaskbarListVtbl {
        pub query_interface: unsafe extern "system" fn(*mut std::ffi::c_void, *const u8, *mut *mut std::ffi::c_void) -> i32,
        pub add_ref: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
        pub release: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
        pub hr_init: unsafe extern "system" fn(*mut std::ffi::c_void) -> i32,
        pub add_tab: unsafe extern "system" fn(*mut std::ffi::c_void, HWND) -> i32,
        pub delete_tab: unsafe extern "system" fn(*mut std::ffi::c_void, HWND) -> i32,
        pub activate_tab: unsafe extern "system" fn(*mut std::ffi::c_void, HWND) -> i32,
        pub set_active_alt: unsafe extern "system" fn(*mut std::ffi::c_void, HWND) -> i32,
    }

    #[repr(C)]
    pub struct ITaskbarList {
        pub lp_vtbl: *const ITaskbarListVtbl,
    }

    // CLSID_TaskbarList = 56FDF344-FD6D-11d0-958A-006097C9A090
    pub const CLSID_TASKBAR_LIST: [u8; 16] = [
        0x44, 0xF3, 0xFD, 0x56, 0x6D, 0xFD, 0xD0, 0x11, 0x95, 0x8A, 0x00, 0x60, 0x97, 0xC9, 0xA0, 0x90,
    ];
    // IID_ITaskbarList = 56FDF342-FD6D-11d0-958A-006097C9A090
    pub const IID_ITASKBAR_LIST: [u8; 16] = [
        0x42, 0xF3, 0xFD, 0x56, 0x6D, 0xFD, 0xD0, 0x11, 0x95, 0x8A, 0x00, 0x60, 0x97, 0xC9, 0xA0, 0x90,
    ];

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub struct RECT {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub struct POINT {
        pub x: i32,
        pub y: i32,
    }

    pub const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;

    extern "system" {
        pub fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> HWND;
        pub fn SetParent(hWndChild: HWND, hWndNewParent: HWND) -> HWND;
        pub fn SetWindowLongPtrW(hWnd: HWND, nIndex: i32, dwNewLong: LONG_PTR) -> LONG_PTR;
        pub fn GetWindowLongPtrW(hWnd: HWND, nIndex: i32) -> LONG_PTR;
        pub fn GetWindowRect(hWnd: HWND, lpRect: *mut RECT) -> BOOL;
        pub fn GetClientRect(hWnd: HWND, lpRect: *mut RECT) -> BOOL;
        pub fn ClientToScreen(hWnd: HWND, lpPoint: *mut POINT) -> BOOL;
        pub fn SetWindowPos(
            hWnd: HWND,
            hWndInsertAfter: HWND,
            X: i32,
            Y: i32,
            cx: i32,
            cy: i32,
            uFlags: u32,
        ) -> BOOL;
        pub fn ShowWindow(hWnd: HWND, nCmdShow: i32) -> BOOL;
        pub fn IsIconic(hWnd: HWND) -> BOOL;
        pub fn IsZoomed(hWnd: HWND) -> BOOL;
        pub fn SetWindowDisplayAffinity(hWnd: HWND, dwAffinity: u32) -> BOOL;
        pub fn EnableWindow(hWnd: HWND, bEnable: BOOL) -> BOOL;
        pub fn SetForegroundWindow(hWnd: HWND) -> BOOL;
        pub fn GetForegroundWindow() -> HWND;
        pub fn SetActiveWindow(hWnd: HWND) -> HWND;
        pub fn BringWindowToTop(hWnd: HWND) -> BOOL;

        pub fn CoInitialize(pvReserved: *mut std::ffi::c_void) -> i32;
        pub fn CoCreateInstance(
            rclsid: *const u8,
            pUnkOuter: *mut std::ffi::c_void,
            dwClsContext: u32,
            riid: *const u8,
            ppv: *mut *mut std::ffi::c_void,
        ) -> i32;

        pub fn SetWindowRgn(hWnd: HWND, hRgn: HRGN, bRedraw: BOOL) -> i32;
        pub fn MonitorFromWindow(hwnd: HWND, dwFlags: u32) -> HMONITOR;
        pub fn GetMonitorInfoW(hMonitor: HMONITOR, lpmi: *mut MONITORINFO) -> BOOL;

        pub fn RegisterClassExW(lpwcx: *const WNDCLASSEXW) -> u16;
        pub fn CreateWindowExW(
            dwExStyle: u32,
            lpClassName: *const u16,
            lpWindowName: *const u16,
            dwStyle: u32,
            X: i32,
            Y: i32,
            nWidth: i32,
            nHeight: i32,
            hWndParent: HWND,
            hMenu: HMENU,
            hInstance: HINSTANCE,
            lpParam: *mut std::ffi::c_void,
        ) -> HWND;
        pub fn DestroyWindow(hWnd: HWND) -> BOOL;
        pub fn DefWindowProcW(hWnd: HWND, Msg: u32, wParam: usize, lParam: isize) -> isize;
        pub fn CallWindowProcW(lpPrevWndFunc: WNDPROC, hWnd: HWND, Msg: u32, wParam: usize, lParam: isize) -> isize;
        pub fn PostMessageW(hWnd: HWND, Msg: u32, wParam: usize, lParam: isize) -> BOOL;
        pub fn IsWindow(hWnd: HWND) -> BOOL;
        pub fn IsWindowVisible(hWnd: HWND) -> BOOL;
        pub fn GetModuleHandleW(lpModuleName: *const u16) -> HMODULE;
        pub fn FillRect(hDC: HDC, lprc: *const RECT, hbr: HBRUSH) -> i32;
        pub fn GetDC(hWnd: HWND) -> HDC;
        pub fn ReleaseDC(hWnd: HWND, hDC: HDC) -> i32;
        pub fn PrintWindow(hwnd: HWND, hdcBlt: HDC, nFlags: u32) -> BOOL;
    }

    pub type HMODULE = *mut c_void;
    pub type HINSTANCE = *mut c_void;
    pub type HMENU = *mut c_void;
    pub type HCURSOR = *mut c_void;
    pub type HICON = *mut c_void;
    pub type HBRUSH = *mut c_void;
    pub type HDC = *mut c_void;
    pub const BLACK_BRUSH: i32 = 4;
    pub type WNDPROC = Option<unsafe extern "system" fn(HWND, u32, usize, isize) -> isize>;

    #[repr(C)]
    #[allow(non_snake_case)]
    pub struct WNDCLASSEXW {
        pub cbSize: u32,
        pub style: u32,
        pub lpfnWndProc: WNDPROC,
        pub cbClsExtra: i32,
        pub cbWndExtra: i32,
        pub hInstance: HINSTANCE,
        pub hIcon: HICON,
        pub hCursor: HCURSOR,
        pub hbrBackground: HBRUSH,
        pub lpszMenuName: *const u16,
        pub lpszClassName: *const u16,
        pub hIconSm: HICON,
    }

    pub type HTHUMBNAIL = *mut c_void;

    pub const DWM_TNP_RECTDESTINATION: u32 = 0x00000001;
    pub const DWM_TNP_RECTSOURCE: u32 = 0x00000002;
    pub const DWM_TNP_OPACITY: u32 = 0x00000004;
    pub const DWM_TNP_VISIBLE: u32 = 0x00000008;
    pub const DWM_TNP_SOURCECLIENTAREAONLY: u32 = 0x00000010;

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    #[allow(non_snake_case)]
    pub struct DWM_THUMBNAIL_PROPERTIES {
        pub dwFlags: u32,
        pub rcDestination: RECT,
        pub rcSource: RECT,
        pub opacity: u8,
        pub fVisible: BOOL,
        pub fSourceClientAreaOnly: BOOL,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    pub struct SIZE {
        pub cx: i32,
        pub cy: i32,
    }

    pub type HMONITOR = *mut std::ffi::c_void;
    pub const MONITOR_DEFAULTTONEAREST: u32 = 2;

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    #[allow(non_snake_case)]
    pub struct MONITORINFO {
        pub cbSize: u32,
        pub rcMonitor: RECT,
        pub rcWork: RECT,
        pub dwFlags: u32,
    }

    pub type HRGN = *mut std::ffi::c_void;
    pub type HGDIOBJ = *mut std::ffi::c_void;

    #[link(name = "gdi32")]
    extern "system" {
        pub fn CreateRoundRectRgn(
            x1: i32,
            y1: i32,
            x2: i32,
            y2: i32,
            w: i32,
            h: i32,
        ) -> HRGN;
        pub fn CreateRectRgn(x1: i32, y1: i32, x2: i32, y2: i32) -> HRGN;
        pub fn DeleteObject(ho: HGDIOBJ) -> BOOL;
        pub fn GetStockObject(i: i32) -> HGDIOBJ;
        pub fn CreateCompatibleDC(hdc: HDC) -> HDC;
        pub fn DeleteDC(hdc: HDC) -> BOOL;
        pub fn CreateCompatibleBitmap(hdc: HDC, cx: i32, cy: i32) -> HGDIOBJ;
        pub fn SelectObject(hdc: HDC, h: HGDIOBJ) -> HGDIOBJ;
        pub fn BitBlt(
            hdcDest: HDC,
            nXDest: i32,
            nYDest: i32,
            nWidth: i32,
            nHeight: i32,
            hdcSrc: HDC,
            nXSrc: i32,
            nYSrc: i32,
            dwRop: u32,
        ) -> BOOL;
        pub fn StretchBlt(
            hdcDest: HDC,
            xDest: i32,
            yDest: i32,
            wDest: i32,
            hDest: i32,
            hdcSrc: HDC,
            xSrc: i32,
            ySrc: i32,
            wSrc: i32,
            hSrc: i32,
            rop: u32,
        ) -> BOOL;
        pub fn SetStretchBltMode(hdc: HDC, mode: i32) -> i32;
        pub fn GetPixel(hdc: HDC, x: i32, y: i32) -> u32;
        pub fn GdiFlush() -> BOOL;
        pub fn CreateDIBSection(
            hdc: HDC,
            pbmi: *const std::ffi::c_void,
            usage: u32,
            ppvBits: *mut *mut std::ffi::c_void,
            hSection: *mut std::ffi::c_void,
            offset: u32,
        ) -> HGDIOBJ;
        pub fn SetBrushOrgEx(hdc: HDC, x: i32, y: i32, lppt: *mut POINT) -> BOOL;
        pub fn GetDIBits(
            hdc: HDC,
            hbm: HGDIOBJ,
            start: u32,
            cLines: u32,
            lpvBits: *mut std::ffi::c_void,
            lpbmi: *mut std::ffi::c_void,
            usage: u32,
        ) -> i32;
    }

    #[link(name = "dwmapi")]
    extern "system" {
        pub fn DwmGetWindowAttribute(
            hwnd: HWND,
            dwAttribute: u32,
            pvAttribute: *mut std::ffi::c_void,
            cbAttribute: u32,
        ) -> i32;
        pub fn DwmFlush() -> i32;
        pub fn DwmSetWindowAttribute(
            hwnd: HWND,
            dwAttribute: u32,
            pvAttribute: *const std::ffi::c_void,
            cbAttribute: u32,
        ) -> i32;
        pub fn DwmRegisterThumbnail(
            hwndDestination: HWND,
            hwndSource: HWND,
            phThumbnailId: *mut HTHUMBNAIL,
        ) -> i32;
        pub fn DwmUnregisterThumbnail(hThumbnailId: HTHUMBNAIL) -> i32;
        pub fn DwmSetIconicThumbnail(hwnd: HWND, hbmp: HGDIOBJ, dwSITFlags: u32) -> i32;
        pub fn DwmSetIconicLivePreviewBitmap(
            hwnd: HWND,
            hbmp: HGDIOBJ,
            pptClient: *const POINT,
            dwSITFlags: u32,
        ) -> i32;
        pub fn DwmInvalidateIconicBitmaps(hwnd: HWND) -> i32;
        pub fn DwmUpdateThumbnailProperties(
            hThumbnailId: HTHUMBNAIL,
            ptnProperties: *const DWM_THUMBNAIL_PROPERTIES,
        ) -> i32;
        pub fn DwmQueryThumbnailSourceSize(
            hThumbnailId: HTHUMBNAIL,
            pSize: *mut SIZE,
        ) -> i32;
    }

    pub const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    pub const DWMWCP_DONOTROUND: u32 = 1;
    pub const DWMWCP_ROUND: u32 = 2;
    pub const DWMWA_BORDER_COLOR: u32 = 34;
    pub const DWMWA_COLOR_NONE: u32 = 0xFFFFFFFE;
    pub const DWMWA_NCRENDERING_POLICY: u32 = 2;
    pub const DWMWA_FORCE_ICONIC_REPRESENTATION: u32 = 7;
    pub const DWMWA_HAS_ICONIC_BITMAP: u32 = 10;
    pub const DWMNCRP_DISABLED: u32 = 1;

    pub fn get_visible_window_rect(hwnd: HWND) -> (i32, i32, u32, u32) {
        let mut pt = POINT { x: 0, y: 0 };
        let mut client_rect = RECT::default();
        let is_max = unsafe { IsZoomed(hwnd) != 0 };
        unsafe {
            ClientToScreen(hwnd, &mut pt);
            GetClientRect(hwnd, &mut client_rect);
        }
        let w = (client_rect.right - client_rect.left).max(100) as u32;
        let h = (client_rect.bottom - client_rect.top).max(100) as u32;

        let inset = if is_max { 0 } else { 1 };
        let x = pt.x + inset;
        let y = pt.y + inset;
        let final_w = w.saturating_sub((inset * 2) as u32);
        let final_h = h.saturating_sub((inset * 2) as u32);
        (x, y, final_w, final_h)
    }

    pub fn delete_taskbar_tab(hwnd: HWND) {
        unsafe {
            let _ = CoInitialize(std::ptr::null_mut());
            let mut tbl_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
            let hr = CoCreateInstance(
                CLSID_TASKBAR_LIST.as_ptr(),
                std::ptr::null_mut(),
                1, // CLSCTX_INPROC_SERVER
                IID_ITASKBAR_LIST.as_ptr(),
                &mut tbl_ptr,
            );
            if hr == 0 && !tbl_ptr.is_null() {
                let tbl = &*(tbl_ptr as *mut ITaskbarList);
                ((*tbl.lp_vtbl).hr_init)(tbl_ptr);
                ((*tbl.lp_vtbl).delete_tab)(tbl_ptr, hwnd);
                ((*tbl.lp_vtbl).release)(tbl_ptr);
            }
        }
    }

    pub fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

static WE_WINDOW_HWND: Mutex<Option<isize>> = Mutex::new(None);
static UNDERLAY_HWND: Mutex<Option<isize>> = Mutex::new(None);
static WE_THUMBNAIL_ID: Mutex<Option<isize>> = Mutex::new(None);
static UNDERLAY_CLASS_REGISTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static WE_TRANSFORM: Mutex<(f32, f32, f32)> = Mutex::new((1.0, 0.0, 0.0));
const WE_WINDOW_TITLE: &str = "ModComparer_BG";

/// Bumped on every play/stop. Delayed follow-up threads (geometry syncs, property
/// resends) capture it and bail out once a newer scene has taken over, so they never
/// touch the next scene's window with the previous scene's data.
static WE_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn bump_we_generation() -> u64 {
    WE_GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
}

fn is_current_we_generation(gen: u64) -> bool {
    WE_GENERATION.load(std::sync::atomic::Ordering::SeqCst) == gen
}

static CURRENT_WE_TITLE: Mutex<String> = Mutex::new(String::new());
static CURRENT_WE_PATH: Mutex<String> = Mutex::new(String::new());

fn get_we_window_hwnd() -> Option<isize> {
    WE_WINDOW_HWND.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn set_we_window_hwnd(hwnd: Option<isize>) {
    let mut lock = WE_WINDOW_HWND.lock().unwrap_or_else(|e| e.into_inner());
    *lock = hwnd;
}

fn take_we_window_hwnd() -> Option<isize> {
    let mut lock = WE_WINDOW_HWND.lock().unwrap_or_else(|e| e.into_inner());
    lock.take()
}

fn get_underlay_hwnd() -> Option<isize> {
    UNDERLAY_HWND.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[allow(dead_code)]
fn set_underlay_hwnd(hwnd: Option<isize>) {
    let mut lock = UNDERLAY_HWND.lock().unwrap_or_else(|e| e.into_inner());
    *lock = hwnd;
}

fn take_underlay_hwnd() -> Option<isize> {
    let mut lock = UNDERLAY_HWND.lock().unwrap_or_else(|e| e.into_inner());
    lock.take()
}

fn get_we_thumbnail_id() -> Option<isize> {
    WE_THUMBNAIL_ID.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn set_we_thumbnail_id(id: Option<isize>) {
    let mut lock = WE_THUMBNAIL_ID.lock().unwrap_or_else(|e| e.into_inner());
    *lock = id;
}

fn take_we_thumbnail_id() -> Option<isize> {
    let mut lock = WE_THUMBNAIL_ID.lock().unwrap_or_else(|e| e.into_inner());
    lock.take()
}

fn get_we_transform() -> (f32, f32, f32) {
    *WE_TRANSFORM.lock().unwrap_or_else(|e| e.into_inner())
}

fn set_we_transform(scale: f32, pan_x: f32, pan_y: f32) {
    let mut lock = WE_TRANSFORM.lock().unwrap_or_else(|e| e.into_inner());
    *lock = (scale, pan_x, pan_y);
}

static LAST_WE_GEOM: Mutex<(i32, i32, i32, i32)> = Mutex::new((0, 0, 0, 0));
static LAST_WE_RGN: Mutex<(i32, i32, i32, i32)> = Mutex::new((0, 0, 0, 0));

#[cfg(target_os = "windows")]
unsafe extern "system" fn underlay_wnd_proc(
    hwnd: we_win32::HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    const WM_ERASEBKGND: u32 = 0x0014;
    if msg == WM_ERASEBKGND {
        let mut rc = we_win32::RECT::default();
        we_win32::GetClientRect(hwnd, &mut rc);
        let hdc = wparam as we_win32::HDC;
        let brush = we_win32::GetStockObject(we_win32::BLACK_BRUSH);
        we_win32::FillRect(hdc, &rc, brush as we_win32::HBRUSH);
        return 1;
    }
    we_win32::DefWindowProcW(hwnd, msg, wparam, lparam)
}

static PREV_APP_WNDPROC: Mutex<Option<usize>> = Mutex::new(None);

#[cfg(target_os = "windows")]
unsafe extern "system" fn app_subclass_wndproc(
    hwnd: we_win32::HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    const WM_ACTIVATE: u32 = 0x0006;
    const WM_ACTIVATEAPP: u32 = 0x001C;
    const WM_SHOWWINDOW: u32 = 0x0018;

    if msg == WM_ACTIVATE || msg == WM_ACTIVATEAPP || msg == WM_SHOWWINDOW {
        let is_active = if msg == WM_ACTIVATE {
            (wparam & 0xFFFF) != 0
        } else {
            wparam != 0
        };

        if is_active {
            if let Some(we_hwnd_val) = get_we_window_hwnd() {
                let is_min = we_win32::IsIconic(hwnd) != 0;
                if !is_min {
                    let (scale, pan_x, pan_y) = get_we_transform();
                    update_we_geometry_internal(
                        we_hwnd_val as we_win32::HWND,
                        hwnd,
                        scale,
                        pan_x,
                        pan_y,
                    );
                    let h_val = hwnd as usize;
                    let we_val = we_hwnd_val as usize;
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(60));
                        let (s, px, py) = get_we_transform();
                        update_we_geometry_internal(
                            we_val as we_win32::HWND,
                            h_val as we_win32::HWND,
                            s,
                            px,
                            py,
                        );
                    });
                }
            }
        }
    }

    // Custom taskbar thumbnail / Aero Peek while a scene is active (see
    // set_taskbar_preview_override): DWM asks for the bitmaps with these messages.
    const WM_DWMSENDICONICTHUMBNAIL: u32 = 0x0323;
    const WM_DWMSENDICONICLIVEPREVIEWBITMAP: u32 = 0x0326;
    const WM_SYSCOMMAND: u32 = 0x0112;
    const SC_MINIMIZE: usize = 0xF020;
    if get_we_window_hwnd().is_some() {
        if msg == WM_DWMSENDICONICTHUMBNAIL {
            let max_w = ((lparam >> 16) & 0xFFFF) as i32;
            let max_h = (lparam & 0xFFFF) as i32;
            send_iconic_thumbnail(hwnd, max_w, max_h);
            return 0;
        }
        if msg == WM_DWMSENDICONICLIVEPREVIEWBITMAP {
            send_iconic_live_preview(hwnd);
            return 0;
        }
        if msg == WM_SYSCOMMAND && (wparam & 0xFFF0) == SC_MINIMIZE {
            // Minimizing from the taskbar/system menu: keep a preview of the scene
            // while its window is still visible.
            refresh_taskbar_preview_cache(hwnd);
        }
    }

    let prev = PREV_APP_WNDPROC.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(p) = prev {
        let prev_fn: we_win32::WNDPROC = std::mem::transmute(p);
        we_win32::CallWindowProcW(prev_fn, hwnd, msg, wparam, lparam)
    } else {
        we_win32::DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

pub fn ensure_app_subclassed(app_hwnd: we_win32::HWND) {
    if app_hwnd.is_null() {
        return;
    }
    let mut prev = PREV_APP_WNDPROC.lock().unwrap_or_else(|e| e.into_inner());
    if prev.is_none() {
        unsafe {
            let old = we_win32::SetWindowLongPtrW(
                app_hwnd,
                we_win32::GWLP_WNDPROC,
                app_subclass_wndproc as *const () as isize,
            );
            if old != 0 {
                *prev = Some(old as usize);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn get_or_create_underlay_window(_app_hwnd: we_win32::HWND, x: i32, y: i32, w: u32, h: u32) -> we_win32::HWND {
    let mut lock = UNDERLAY_HWND.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(hwnd_val) = *lock {
        let hwnd = hwnd_val as we_win32::HWND;
        unsafe {
            if we_win32::IsWindow(hwnd) != 0 {
                return hwnd;
            }
        }
    }

    unsafe {
        let class_name = we_win32::to_wide("ModComparerUnderlayClass");
        if !UNDERLAY_CLASS_REGISTERED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let wc = we_win32::WNDCLASSEXW {
                cbSize: std::mem::size_of::<we_win32::WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(underlay_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: we_win32::GetModuleHandleW(std::ptr::null()),
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: we_win32::GetStockObject(we_win32::BLACK_BRUSH) as we_win32::HBRUSH,
                lpszMenuName: std::ptr::null(),
                lpszClassName: class_name.as_ptr(),
                hIconSm: std::ptr::null_mut(),
            };
            we_win32::RegisterClassExW(&wc);
        }

        let title = we_win32::to_wide("ModComparer_Underlay");
        let hwnd = we_win32::CreateWindowExW(
            we_win32::WS_EX_TOOLWINDOW | we_win32::WS_EX_NOACTIVATE | we_win32::WS_EX_TRANSPARENT,
            class_name.as_ptr(),
            title.as_ptr(),
            we_win32::WS_POPUP,
            x, y, w as i32, h as i32,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            we_win32::GetModuleHandleW(std::ptr::null()),
            std::ptr::null_mut(),
        );

        if !hwnd.is_null() {
            we_win32::EnableWindow(hwnd, 0);

            let corner_pref: u32 = we_win32::DWMWCP_ROUND;
            let _ = we_win32::DwmSetWindowAttribute(
                hwnd,
                we_win32::DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner_pref as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );
            let border_color: u32 = we_win32::DWMWA_COLOR_NONE;
            let _ = we_win32::DwmSetWindowAttribute(
                hwnd,
                we_win32::DWMWA_BORDER_COLOR,
                &border_color as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );
            let ncrp_disabled: u32 = we_win32::DWMNCRP_DISABLED;
            let _ = we_win32::DwmSetWindowAttribute(
                hwnd,
                we_win32::DWMWA_NCRENDERING_POLICY,
                &ncrp_disabled as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );
            we_win32::delete_taskbar_tab(hwnd);
            *lock = Some(hwnd as isize);
        }
        hwnd
    }
}

#[cfg(target_os = "windows")]
fn compute_thumbnail_rects(
    src_w: i32,
    src_h: i32,
    dest_w: i32,
    dest_h: i32,
    scale: f32,
    pan_x: f32,
    pan_y: f32,
) -> (we_win32::RECT, we_win32::RECT) {
    let clamped_scale = scale.clamp(0.5, 5.0);
    let sw = src_w.max(10) as f32;
    let sh = src_h.max(10) as f32;
    let dw = dest_w.max(10) as f32;
    let dh = dest_h.max(10) as f32;

    if clamped_scale >= 1.0 {
        let crop_w = (sw / clamped_scale).round() as i32;
        let crop_h = (sh / clamped_scale).round() as i32;
        let crop_w = crop_w.clamp(1, src_w);
        let crop_h = crop_h.clamp(1, src_h);

        let center_x = (sw / 2.0) - (pan_x / clamped_scale);
        let center_y = (sh / 2.0) - (pan_y / clamped_scale);

        let mut l = (center_x - (crop_w as f32 / 2.0)).round() as i32;
        let mut t = (center_y - (crop_h as f32 / 2.0)).round() as i32;

        if l < 0 {
            l = 0;
        }
        if l + crop_w > src_w {
            l = (src_w - crop_w).max(0);
        }
        if t < 0 {
            t = 0;
        }
        if t + crop_h > src_h {
            t = (src_h - crop_h).max(0);
        }

        let rc_source = we_win32::RECT {
            left: l,
            top: t,
            right: l + crop_w,
            bottom: t + crop_h,
        };
        let rc_dest = we_win32::RECT {
            left: 0,
            top: 0,
            right: dest_w,
            bottom: dest_h,
        };
        (rc_source, rc_dest)
    } else {
        let target_dest_w = (dw * clamped_scale).round() as i32;
        let target_dest_h = (dh * clamped_scale).round() as i32;
        let dl = (((dw - target_dest_w as f32) / 2.0) + pan_x).round() as i32;
        let dt = (((dh - target_dest_h as f32) / 2.0) + pan_y).round() as i32;

        let rc_source = we_win32::RECT {
            left: 0,
            top: 0,
            right: src_w,
            bottom: src_h,
        };
        let rc_dest = we_win32::RECT {
            left: dl,
            top: dt,
            right: dl + target_dest_w,
            bottom: dt + target_dest_h,
        };
        (rc_source, rc_dest)
    }
}

#[cfg(target_os = "windows")]
fn update_we_geometry_internal(
    hwnd: we_win32::HWND,
    app_hwnd: we_win32::HWND,
    scale: f32,
    pan_x: f32,
    pan_y: f32,
) {
    if hwnd.is_null() || app_hwnd.is_null() {
        return;
    }
    thaw_we_scene_before_window_ops();
    let (win_x, win_y, win_w, win_h) = we_win32::get_visible_window_rect(app_hwnd);
    let is_min = unsafe { we_win32::IsIconic(app_hwnd) != 0 }
        || win_x < -10000
        || win_y < -10000
        || win_w < 50
        || win_h < 50;

    if is_min {
        unsafe {
            we_win32::ShowWindow(hwnd, we_win32::SW_HIDE);
            if let Some(u_val) = get_underlay_hwnd() {
                we_win32::ShowWindow(u_val as we_win32::HWND, we_win32::SW_HIDE);
            }
        }
        *LAST_WE_GEOM.lock().unwrap_or_else(|e| e.into_inner()) = (0, 0, 0, 0);
        return;
    }

    ensure_app_subclassed(app_hwnd);
    let underlay = get_or_create_underlay_window(app_hwnd, win_x, win_y, win_w, win_h);

    *LAST_WE_GEOM.lock().unwrap_or_else(|e| e.into_inner()) = (win_x, win_y, win_w as i32, win_h as i32);
    // Check both windows separately: when switching scenes the underlay is already
    // visible but the new WE window was just hidden by prepare_we_window.
    let is_hidden = unsafe { we_win32::IsWindowVisible(underlay) == 0 };
    let show_flag = if is_hidden { we_win32::SWP_SHOWWINDOW } else { 0 };
    let we_hidden = unsafe { we_win32::IsWindowVisible(hwnd) == 0 };
    let we_show_flag = if we_hidden { we_win32::SWP_SHOWWINDOW } else { 0 };

    unsafe {
        if is_hidden {
            we_win32::ShowWindow(underlay, we_win32::SW_SHOWNOACTIVATE);
        }
        if we_hidden {
            we_win32::ShowWindow(hwnd, we_win32::SW_SHOWNOACTIVATE);
        }

            // 1. Keep app_hwnd at HWND_TOP, but only while the user is actually in the
            //    launcher: raising it otherwise covers whatever app they are using
            //    (e.g. on every playlist rotation). Steps 2-3 chain the underlay and
            //    the WE window right behind app_hwnd either way.
            if we_win32::GetForegroundWindow() == app_hwnd {
                we_win32::SetWindowPos(
                    app_hwnd,
                    0 as we_win32::HWND, // HWND_TOP
                    0, 0, 0, 0,
                    we_win32::SWP_NOMOVE | we_win32::SWP_NOSIZE | we_win32::SWP_NOACTIVATE,
                );
            }

            // 2. Position underlay window right behind app_hwnd matching launcher visible bounds
            we_win32::SetWindowPos(
                underlay,
                app_hwnd,
                win_x,
                win_y,
                win_w as i32,
                win_h as i32,
                we_win32::SWP_NOACTIVATE | show_flag,
            );

            // 3. Position found_hwnd deeply inset (60px inside on all 4 borders) right behind underlay
            let inset = 60;
            let hwnd_x = win_x + inset;
            let hwnd_y = win_y + inset;
            let hwnd_w = (win_w as i32 - (inset * 2)).max(100);
            let hwnd_h = (win_h as i32 - (inset * 2)).max(100);

            we_win32::SetWindowPos(
                hwnd,
                underlay,
                hwnd_x,
                hwnd_y,
                hwnd_w,
                hwnd_h,
                we_win32::SWP_NOACTIVATE | we_show_flag,
            );

            // 4. Update region only if dimensions changed to avoid stutter during drag
            let mut last_rgn = LAST_WE_RGN.lock().unwrap_or_else(|e| e.into_inner());
            if *last_rgn != (0, 0, win_w as i32, win_h as i32) {
                *last_rgn = (0, 0, win_w as i32, win_h as i32);
                let is_zoomed = we_win32::IsZoomed(app_hwnd) != 0;
                let corner_pref: u32 = if is_zoomed {
                    we_win32::DWMWCP_DONOTROUND
                } else {
                    we_win32::DWMWCP_ROUND
                };
                let _ = we_win32::DwmSetWindowAttribute(
                    underlay,
                    we_win32::DWMWA_WINDOW_CORNER_PREFERENCE,
                    &corner_pref as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                );

                if is_zoomed {
                    we_win32::SetWindowRgn(underlay, std::ptr::null_mut(), 1);
                } else {
                    // Clip underlay with 16px rounded corners
                    let hrgn = we_win32::CreateRoundRectRgn(0, 0, win_w as i32, win_h as i32, 16, 16);
                    if !hrgn.is_null() {
                        let res = we_win32::SetWindowRgn(underlay, hrgn, 1);
                        if res == 0 {
                            we_win32::DeleteObject(hrgn as we_win32::HGDIOBJ);
                        }
                    }
                }
            }
        }

    // Register thumbnail if not yet registered
    let thumb_opt = get_we_thumbnail_id();
    let thumb_id = if let Some(tid) = thumb_opt {
        tid as we_win32::HTHUMBNAIL
    } else {
        let mut new_thumb: we_win32::HTHUMBNAIL = std::ptr::null_mut();
        let hr = unsafe { we_win32::DwmRegisterThumbnail(underlay, hwnd, &mut new_thumb) };
        if hr == 0 && !new_thumb.is_null() {
            set_we_thumbnail_id(Some(new_thumb as isize));
            new_thumb
        } else {
            std::ptr::null_mut()
        }
    };

    if !thumb_id.is_null() {
        let mut src_size = we_win32::SIZE { cx: win_w as i32, cy: win_h as i32 };
        unsafe {
            let _ = we_win32::DwmQueryThumbnailSourceSize(thumb_id, &mut src_size);
        }
        let src_w = if src_size.cx > 0 { src_size.cx } else { win_w as i32 };
        let src_h = if src_size.cy > 0 { src_size.cy } else { win_h as i32 };

        let (src_rect, dest_rect) = compute_thumbnail_rects(
            src_w,
            src_h,
            win_w as i32,
            win_h as i32,
            scale,
            pan_x,
            pan_y,
        );

        let props = we_win32::DWM_THUMBNAIL_PROPERTIES {
            dwFlags: we_win32::DWM_TNP_RECTDESTINATION
                | we_win32::DWM_TNP_RECTSOURCE
                | we_win32::DWM_TNP_VISIBLE
                | we_win32::DWM_TNP_SOURCECLIENTAREAONLY
                | we_win32::DWM_TNP_OPACITY,
            rcDestination: dest_rect,
            rcSource: src_rect,
            opacity: 255,
            fVisible: 1,
            fSourceClientAreaOnly: 1,
        };

        unsafe {
            let _ = we_win32::DwmUpdateThumbnailProperties(thumb_id, &props);
        }
    }
}

fn ensure_wallpaper_engine_running(we_exe: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let running = std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq wallpaper64.exe", "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("wallpaper64.exe"))
            .unwrap_or(false);

        if !running {
            let running_32 = std::process::Command::new("tasklist")
                .args(["/FI", "IMAGENAME eq wallpaper32.exe", "/NH"])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("wallpaper32.exe"))
                .unwrap_or(false);

            if !running_32 {
                let launcher = we_exe.parent().map(|p| p.join("launcher.exe"));
                if let Some(l) = launcher.filter(|p| p.exists()) {
                    let _ = std::process::Command::new(l).spawn();
                } else {
                    let _ = std::process::Command::new(we_exe).spawn();
                }
                std::thread::sleep(std::time::Duration::from_millis(600));
            }
        }
    }
}

fn find_wallpaper_engine_path() -> Option<std::path::PathBuf> {
    let candidates = [
        r"C:\Program Files (x86)\Steam\steamapps\common\wallpaper_engine\wallpaper64.exe",
        r"C:\Program Files\Steam\steamapps\common\wallpaper_engine\wallpaper64.exe",
        r"D:\SteamLibrary\steamapps\common\wallpaper_engine\wallpaper64.exe",
        r"E:\SteamLibrary\steamapps\common\wallpaper_engine\wallpaper64.exe",
        r"C:\Program Files (x86)\Steam\steamapps\common\wallpaper_engine\wallpaper32.exe",
        r"C:\Program Files\Steam\steamapps\common\wallpaper_engine\wallpaper32.exe",
    ];
    for c in candidates {
        let p = std::path::PathBuf::from(c);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

#[tauri::command(async)]
fn find_wallpaper_engine_exe() -> Option<String> {
    find_wallpaper_engine_path().map(|p| p.to_string_lossy().to_string())
}

fn resolve_we_target_file(project_path: &str) -> std::path::PathBuf {
    let path = std::path::Path::new(project_path);
    if path.is_file() {
        if path.file_name().and_then(|s| s.to_str()) == Some("scene.pkg") {
            let pj = path.parent().map(|p| p.join("project.json")).unwrap_or_else(|| path.to_path_buf());
            if pj.exists() { pj } else { path.to_path_buf() }
        } else {
            path.to_path_buf()
        }
    } else if path.is_dir() {
        let pj = path.join("project.json");
        if pj.exists() { pj } else { path.to_path_buf() }
    } else {
        path.to_path_buf()
    }
}

fn same_we_project(a: &str, b: &str) -> bool {
    let norm = |p: &str| {
        let f = resolve_we_target_file(p);
        let dir = if f.is_dir() { f } else { f.parent().map(|d| d.to_path_buf()).unwrap_or(f) };
        dir.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_lowercase()
    };
    norm(a) == norm(b)
}

fn clean_we_properties(props: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut clean = serde_json::Map::new();
    if let Some(map) = props.as_object() {
        for (k, v) in map {
            if !k.eq_ignore_ascii_case("schemecolor") {
                clean.insert(k.clone(), v.clone());
            }
        }
    }
    clean
}

/// Sends `props` live to the -playInWindow window titled `title`.
///
/// Wallpaper Engine reads the JSON straight from the raw command line between
/// `RAW~(` and `)~END`, so the quotes must NOT be escaped: `Command::arg` would turn
/// them into `\"` and WE silently ignores the (now invalid) JSON. Spaces inside the
/// payload (e.g. color values) are fine unquoted.
fn send_we_properties(
    we_exe: &std::path::Path,
    title: &str,
    props: &serde_json::Map<String, serde_json::Value>,
) {
    if title.is_empty() || props.is_empty() {
        return;
    }
    let raw_payload = format!("RAW~({})~END", serde_json::Value::Object(props.clone()));
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let _ = std::process::Command::new(we_exe)
            .args(["-control", "applyProperties", "-properties"])
            .raw_arg(&raw_payload)
            .args(["-location", title])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (we_exe, raw_payload);
}

/// Asks the running Wallpaper Engine to open `target_file` in a window titled `title`.
/// (`openWallpaper -properties` is ignored by WE; options are sent afterwards with
/// `send_we_properties`.)
fn spawn_we_window(
    we_exe: &std::path::Path,
    target_file: &std::path::Path,
    title: &str,
    (x, y, w, h): (i32, i32, u32, u32),
) -> Result<(), String> {
    let mut cmd = std::process::Command::new(we_exe);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.args([
        "-control",
        "openWallpaper",
        "-file",
        &target_file.to_string_lossy(),
        "-playInWindow",
        title,
        "-width",
        &w.to_string(),
        "-height",
        &h.to_string(),
        "-x",
        &x.to_string(),
        "-y",
        &y.to_string(),
        "-borderless",
    ]);
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Error ejecutando Wallpaper Engine: {}", e))
}

#[cfg(target_os = "windows")]
fn we_window_geometry(app_hwnd: we_win32::HWND) -> (i32, i32, u32, u32) {
    let (x, y, width, height) = we_win32::get_visible_window_rect(app_hwnd);
    let inset = 60;
    (
        x + inset,
        y + inset,
        width.saturating_sub((inset * 2) as u32).max(100),
        height.saturating_sub((inset * 2) as u32).max(100),
    )
}

/// Waits for the new -playInWindow window. `stale` is the previous scene's window:
/// if it is somehow still alive it shares the title, so it must never be mistaken
/// for the new one.
#[cfg(target_os = "windows")]
fn wait_for_we_window(title: &str, stale: Option<isize>) -> we_win32::HWND {
    let wide_title = we_win32::to_wide(title);
    for _ in 0..170 {
        std::thread::sleep(std::time::Duration::from_millis(30));
        let hwnd = unsafe { we_win32::FindWindowW(std::ptr::null(), wide_title.as_ptr()) };
        if !hwnd.is_null() && Some(hwnd as isize) != stale {
            return hwnd;
        }
    }
    std::ptr::null_mut()
}

/// Closes the scene playing in the launcher window and blocks until Wallpaper Engine
/// has fully destroyed it, so the next scene is only opened once the old one is gone.
#[cfg(target_os = "windows")]
fn close_we_window_and_wait(we_exe: &std::path::Path, old_hwnd: Option<isize>) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let wide_title = we_win32::to_wide(WE_WINDOW_TITLE);
    let is_gone = || unsafe {
        let old_alive = old_hwnd.map_or(false, |h| we_win32::IsWindow(h as we_win32::HWND) != 0);
        !old_alive && we_win32::FindWindowW(std::ptr::null(), wide_title.as_ptr()).is_null()
    };
    if is_gone() {
        return;
    }

    // Retry the close once in case WE was still busy loading the old scene.
    for _attempt in 0..2 {
        let _ = std::process::Command::new(we_exe)
            .args(["-control", "closeWallpaper", "-location", WE_WINDOW_TITLE])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
        for _ in 0..120 {
            if is_gone() {
                // Let WE release the scene/CEF resources before opening the next one.
                std::thread::sleep(std::time::Duration::from_millis(300));
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
    eprintln!("[WE] la ventana anterior no se cerró a tiempo; se abre la nueva igualmente");
}

/// Hides the freshly created WE window and strips it down so it can only be seen
/// through the DWM thumbnail on the underlay.
#[cfg(target_os = "windows")]
unsafe fn prepare_we_window(found_hwnd: we_win32::HWND) {
    // 1. Hide initially while configuring
    we_win32::ShowWindow(found_hwnd, we_win32::SW_HIDE);

    // 2. Ensure found_hwnd has NO owner so Windows doesn't force it on top of app_hwnd
    we_win32::SetWindowLongPtrW(found_hwnd, we_win32::GWLP_HWNDPARENT, 0);

    // 3. Strip WS_BORDER, WS_THICKFRAME, WS_CAPTION, WS_MINIMIZEBOX, WS_MAXIMIZEBOX, WS_SYSMENU from GWL_STYLE
    let cur_style = we_win32::GetWindowLongPtrW(found_hwnd, we_win32::GWL_STYLE) as u32;
    let new_style = (cur_style & !(0x00C00000 | 0x00040000 | 0x00020000 | 0x00010000 | 0x00080000 | 0x20000000)) | 0x80000000;
    we_win32::SetWindowLongPtrW(found_hwnd, we_win32::GWL_STYLE, new_style as isize);

    // 4. Strip WS_EX_APPWINDOW, add WS_EX_TOOLWINDOW + WS_EX_NOACTIVATE + WS_EX_TRANSPARENT
    let cur_ex = we_win32::GetWindowLongPtrW(found_hwnd, we_win32::GWL_EXSTYLE) as u32;
    let new_ex = (cur_ex & !we_win32::WS_EX_APPWINDOW)
        | we_win32::WS_EX_TOOLWINDOW
        | we_win32::WS_EX_NOACTIVATE
        | we_win32::WS_EX_TRANSPARENT;
    we_win32::SetWindowLongPtrW(found_hwnd, we_win32::GWL_EXSTYLE, new_ex as isize);

    // 5. Disable input so clicks fall straight through to WebView2 buttons
    we_win32::EnableWindow(found_hwnd, 0);

    // 6. Suppress outer DWM borders and round corners matching Windows 11 launcher window
    let corner_pref: u32 = we_win32::DWMWCP_ROUND;
    let _ = we_win32::DwmSetWindowAttribute(
        found_hwnd,
        we_win32::DWMWA_WINDOW_CORNER_PREFERENCE,
        &corner_pref as *const _ as *const std::ffi::c_void,
        std::mem::size_of::<u32>() as u32,
    );
    let border_color: u32 = we_win32::DWMWA_COLOR_NONE;
    let _ = we_win32::DwmSetWindowAttribute(
        found_hwnd,
        we_win32::DWMWA_BORDER_COLOR,
        &border_color as *const _ as *const std::ffi::c_void,
        std::mem::size_of::<u32>() as u32,
    );
    let ncrp_disabled: u32 = we_win32::DWMNCRP_DISABLED;
    let _ = we_win32::DwmSetWindowAttribute(
        found_hwnd,
        we_win32::DWMWA_NCRENDERING_POLICY,
        &ncrp_disabled as *const _ as *const std::ffi::c_void,
        std::mem::size_of::<u32>() as u32,
    );

    // 7. Call ITaskbarList DeleteTab explicitly
    we_win32::delete_taskbar_tab(found_hwnd);
}

#[tauri::command(async)]
fn get_we_preview_path(project_path: String) -> Option<String> {
    let p = std::path::PathBuf::from(&project_path);
    let folder = if p.is_dir() {
        p
    } else {
        p.parent().map(|d| d.to_path_buf()).unwrap_or(p)
    };

    for name in ["preview.gif", "preview.jpg", "preview.png", "preview.mp4"] {
        let f = folder.join(name);
        if f.exists() {
            return Some(f.to_string_lossy().to_string());
        }
    }
    None
}

/// Freeze-frame of the current scene, laid out exactly like the live background so the
/// frontend can show it 1:1 (object-fit: fill over the whole viewport) without any jump.
///
/// The returned image has the size of the launcher client area and contains what the
/// user sees behind the WebView: the underlay with the DWM thumbnail (zoom/pan and
/// stretch included), placed at the same 1px inset.
///
/// Returned as raw binary (8-byte header: width, height as u32 LE, then top-down BGRA
/// rows) instead of a base64 BMP data URL: the snapshot must reach the screen within a
/// frame or two, otherwise it shows a frame older than the one being frozen.
#[tauri::command]
fn capture_we_snapshot(window: tauri::WebviewWindow) -> Result<tauri::ipc::Response, String> {
    capture_we_snapshot_raw(window)
        .map(tauri::ipc::Response::new)
        .ok_or_else(|| "No se pudo capturar el fondo".to_string())
}

fn capture_we_snapshot_raw(window: tauri::WebviewWindow) -> Option<Vec<u8>> {
    #[cfg(target_os = "windows")]
    unsafe {
        let app_hwnd = window.hwnd().ok()?.0 as we_win32::HWND;
        let hwnd_val = get_we_window_hwnd()?;
        let hwnd = hwnd_val as we_win32::HWND;
        if we_win32::IsWindow(hwnd) == 0 || app_hwnd.is_null() {
            return None;
        }

        // Output = launcher client area; background = underlay rect inside it.
        let mut app_rc = we_win32::RECT::default();
        we_win32::GetClientRect(app_hwnd, &mut app_rc);
        let cw = app_rc.right - app_rc.left;
        let ch = app_rc.bottom - app_rc.top;
        let (bg_x, bg_y, bg_w, bg_h) = we_win32::get_visible_window_rect(app_hwnd);
        let mut app_origin = we_win32::POINT { x: 0, y: 0 };
        we_win32::ClientToScreen(app_hwnd, &mut app_origin);
        let off_x = bg_x - app_origin.x;
        let off_y = bg_y - app_origin.y;
        let (bg_w, bg_h) = (bg_w as i32, bg_h as i32);
        if cw < 10 || ch < 10 || bg_w < 10 || bg_h < 10 {
            return None;
        }

        let hdc_screen = we_win32::GetDC(std::ptr::null_mut());
        if hdc_screen.is_null() {
            return None;
        }
        let hdc_out = we_win32::CreateCompatibleDC(hdc_screen);
        let hbm_out = we_win32::CreateCompatibleBitmap(hdc_screen, cw, ch);
        if hdc_out.is_null() || hbm_out.is_null() {
            if !hbm_out.is_null() {
                we_win32::DeleteObject(hbm_out);
            }
            if !hdc_out.is_null() {
                we_win32::DeleteDC(hdc_out);
            }
            we_win32::ReleaseDC(std::ptr::null_mut(), hdc_screen);
            return None;
        }
        let old_out = we_win32::SelectObject(hdc_out, hbm_out);
        let full = we_win32::RECT { left: 0, top: 0, right: cw, bottom: ch };
        we_win32::FillRect(hdc_out, &full, we_win32::GetStockObject(we_win32::BLACK_BRUSH) as we_win32::HBRUSH);

        const SRCCOPY: u32 = 0x00CC0020;
        const HALFTONE: i32 = 4;
        const WDA_NONE: u32 = 0;
        const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;
        let has_content = |hdc: we_win32::HDC, x: i32, y: i32, w: i32, h: i32| -> bool {
            // Sample a grid; a scene that is still loading is plain black.
            for gy in 1..8 {
                for gx in 1..8 {
                    let c = we_win32::GetPixel(hdc, x + w * gx / 8, y + h * gy / 8);
                    if c != 0xFFFF_FFFF && (c & 0xFF > 10 || (c >> 8) & 0xFF > 10 || (c >> 16) & 0xFF > 10) {
                        return true;
                    }
                }
            }
            false
        };

        let mut captured = false;

        // 1. Exactly what is on screen behind the launcher: exclude the launcher window
        //    from capture for a moment and copy the underlay rect (DWM already applied
        //    zoom, pan and stretch there).
        let underlay_visible = get_underlay_hwnd()
            .map(|u| we_win32::IsWindowVisible(u as we_win32::HWND) != 0)
            .unwrap_or(false);
        if underlay_visible && we_win32::SetWindowDisplayAffinity(app_hwnd, WDA_EXCLUDEFROMCAPTURE) != 0 {
            // Wait for DWM to compose a frame without the launcher (no fixed sleep:
            // every extra millisecond makes the frozen frame older).
            we_win32::DwmFlush();
            we_win32::BitBlt(hdc_out, off_x, off_y, bg_w, bg_h, hdc_screen, bg_x, bg_y, SRCCOPY);
            we_win32::SetWindowDisplayAffinity(app_hwnd, WDA_NONE);
            captured = has_content(hdc_out, off_x, off_y, bg_w, bg_h);
        }

        // 2. Fallback: render the WE window and map it with the same rects as the thumbnail.
        //    Never while frozen: PrintWindow would block on the suspended render threads.
        if !captured && !we_freeze::is_frozen() {
            let mut rc = we_win32::RECT::default();
            we_win32::GetClientRect(hwnd, &mut rc);
            let w = rc.right - rc.left;
            let h = rc.bottom - rc.top;
            let hdc_src = we_win32::CreateCompatibleDC(hdc_screen);
            let hbm_src = if hdc_src.is_null() || w < 10 || h < 10 {
                std::ptr::null_mut()
            } else {
                we_win32::CreateCompatibleBitmap(hdc_screen, w, h)
            };
            if !hbm_src.is_null() {
                let old_src = we_win32::SelectObject(hdc_src, hbm_src);
                for flags in [2u32, 0u32] {
                    if we_win32::PrintWindow(hwnd, hdc_src, flags) != 0 && has_content(hdc_src, 0, 0, w, h) {
                        let (scale, pan_x, pan_y) = get_we_transform();
                        let (src, dst) = compute_thumbnail_rects(w, h, bg_w, bg_h, scale, pan_x, pan_y);
                        we_win32::FillRect(hdc_out, &full, we_win32::GetStockObject(we_win32::BLACK_BRUSH) as we_win32::HBRUSH);
                        we_win32::SetStretchBltMode(hdc_out, HALFTONE);
                        we_win32::SetBrushOrgEx(hdc_out, 0, 0, std::ptr::null_mut());
                        we_win32::StretchBlt(
                            hdc_out,
                            off_x + dst.left,
                            off_y + dst.top,
                            dst.right - dst.left,
                            dst.bottom - dst.top,
                            hdc_src,
                            src.left,
                            src.top,
                            src.right - src.left,
                            src.bottom - src.top,
                            SRCCOPY,
                        );
                        captured = true;
                        break;
                    }
                }
                we_win32::SelectObject(hdc_src, old_src);
                we_win32::DeleteObject(hbm_src);
            }
            if !hdc_src.is_null() {
                we_win32::DeleteDC(hdc_src);
            }
        }

        we_win32::SelectObject(hdc_out, old_out);

        let mut raw = None;
        if captured {
            #[repr(C)]
            #[allow(non_snake_case)]
            struct BITMAPINFOHEADER {
                biSize: u32,
                biWidth: i32,
                biHeight: i32,
                biPlanes: u16,
                biBitCount: u16,
                biCompression: u32,
                biSizeImage: u32,
                biXPelsPerMeter: i32,
                biYPelsPerMeter: i32,
                biClrUsed: u32,
                biClrImportant: u32,
            }
            let pixel_bytes = (cw * ch * 4) as usize;
            let mut bmi = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: cw,
                biHeight: -ch, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                biSizeImage: pixel_bytes as u32,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            };
            let mut out = vec![0u8; 8 + pixel_bytes];
            out[0..4].copy_from_slice(&(cw as u32).to_le_bytes());
            out[4..8].copy_from_slice(&(ch as u32).to_le_bytes());
            let lines = we_win32::GetDIBits(
                hdc_screen,
                hbm_out,
                0,
                ch as u32,
                out[8..].as_mut_ptr() as *mut std::ffi::c_void,
                &mut bmi as *mut _ as *mut std::ffi::c_void,
                0, // DIB_RGB_COLORS
            );
            if lines > 0 {
                raw = Some(out);
            }
        }

        we_win32::DeleteObject(hbm_out);
        we_win32::DeleteDC(hdc_out);
        we_win32::ReleaseDC(std::ptr::null_mut(), hdc_screen);
        return raw;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        None
    }
}

// ---------------------------------------------------------------------------
// Taskbar thumbnail / Aero Peek while a WE scene is the background.
//
// The launcher's WebView is transparent and the scene lives in other windows, so the
// preview Windows builds from the launcher alone shows the UI over black. While a scene
// is active we take over the preview (DWMWA_FORCE_ICONIC_REPRESENTATION) and hand DWM
// our own bitmap: the scene mapped like the live background + the UI on top.
// ---------------------------------------------------------------------------

/// Last composed preview (BGRA, top-down, w, h). Used while minimized, when the scene
/// window is hidden and cannot be captured.
static TASKBAR_PREVIEW: Mutex<Option<std::sync::Arc<(Vec<u8>, i32, i32)>>> = Mutex::new(None);
/// Launcher window, so stop_we_wallpaper (which gets no window) can reset the preview.
static PREVIEW_APP_HWND: Mutex<Option<isize>> = Mutex::new(None);

#[cfg(target_os = "windows")]
#[repr(C)]
#[allow(non_snake_case)]
struct DibHeader {
    biSize: u32,
    biWidth: i32,
    biHeight: i32,
    biPlanes: u16,
    biBitCount: u16,
    biCompression: u32,
    biSizeImage: u32,
    biXPelsPerMeter: i32,
    biYPelsPerMeter: i32,
    biClrUsed: u32,
    biClrImportant: u32,
}

/// 32bpp top-down DIB section selected into its own memory DC.
#[cfg(target_os = "windows")]
struct Dib {
    dc: we_win32::HDC,
    bmp: we_win32::HGDIOBJ,
    old: we_win32::HGDIOBJ,
    bits: *mut u8,
    w: i32,
    h: i32,
}

#[cfg(target_os = "windows")]
impl Dib {
    unsafe fn new(w: i32, h: i32) -> Option<Dib> {
        if w <= 0 || h <= 0 {
            return None;
        }
        let dc = we_win32::CreateCompatibleDC(std::ptr::null_mut());
        if dc.is_null() {
            return None;
        }
        let hdr = DibHeader {
            biSize: std::mem::size_of::<DibHeader>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let bmp = we_win32::CreateDIBSection(
            dc,
            &hdr as *const _ as *const std::ffi::c_void,
            0,
            &mut bits,
            std::ptr::null_mut(),
            0,
        );
        if bmp.is_null() || bits.is_null() {
            we_win32::DeleteDC(dc);
            return None;
        }
        let old = we_win32::SelectObject(dc, bmp);
        Some(Dib { dc, bmp, old, bits: bits as *mut u8, w, h })
    }

    unsafe fn from_pixels(px: &[u8], w: i32, h: i32) -> Option<Dib> {
        let dib = Dib::new(w, h)?;
        dib.pixels().copy_from_slice(&px[..(w * h * 4) as usize]);
        Some(dib)
    }

    unsafe fn pixels(&self) -> &mut [u8] {
        we_win32::GdiFlush();
        std::slice::from_raw_parts_mut(self.bits, (self.w * self.h * 4) as usize)
    }

    /// Releases the DC and hands over the bitmap (caller must DeleteObject it).
    unsafe fn into_bitmap(self) -> we_win32::HGDIOBJ {
        we_win32::GdiFlush();
        we_win32::SelectObject(self.dc, self.old);
        we_win32::DeleteDC(self.dc);
        let bmp = self.bmp;
        std::mem::forget(self);
        bmp
    }
}

#[cfg(target_os = "windows")]
impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            we_win32::SelectObject(self.dc, self.old);
            we_win32::DeleteObject(self.bmp);
            we_win32::DeleteDC(self.dc);
        }
    }
}

/// Scene (as laid out behind the launcher) + launcher UI, at client size.
#[cfg(target_os = "windows")]
unsafe fn compose_taskbar_preview(app_hwnd: we_win32::HWND) -> Option<(Vec<u8>, i32, i32)> {
    // Frozen scene: PrintWindow on it would block; the cached preview is used instead.
    if we_win32::IsIconic(app_hwnd) != 0 || we_freeze::is_frozen() {
        return None;
    }
    let we = get_we_window_hwnd()? as we_win32::HWND;
    if we_win32::IsWindow(we) == 0 || we_win32::IsWindowVisible(we) == 0 {
        return None;
    }
    let mut rc = we_win32::RECT::default();
    we_win32::GetClientRect(app_hwnd, &mut rc);
    let (cw, ch) = (rc.right - rc.left, rc.bottom - rc.top);
    let mut wrc = we_win32::RECT::default();
    we_win32::GetClientRect(we, &mut wrc);
    let (sw, sh) = (wrc.right - wrc.left, wrc.bottom - wrc.top);
    if cw < 10 || ch < 10 || sw < 10 || sh < 10 {
        return None;
    }

    // UI: the WebView renders its transparent areas as black here.
    let ui = Dib::new(cw, ch)?;
    if we_win32::PrintWindow(app_hwnd, ui.dc, 3) == 0 {
        return None; // PW_CLIENTONLY | PW_RENDERFULLCONTENT
    }
    // Scene: mapped with the same rects as the live DWM thumbnail (zoom/pan included).
    let src = Dib::new(sw, sh)?;
    if we_win32::PrintWindow(we, src.dc, 2) == 0 {
        return None;
    }
    let bg = Dib::new(cw, ch)?;
    let (bx, by, bw, bh) = we_win32::get_visible_window_rect(app_hwnd);
    let mut origin = we_win32::POINT { x: 0, y: 0 };
    we_win32::ClientToScreen(app_hwnd, &mut origin);
    let (scale, pan_x, pan_y) = get_we_transform();
    let (s, d) = compute_thumbnail_rects(sw, sh, bw as i32, bh as i32, scale, pan_x, pan_y);
    we_win32::SetStretchBltMode(bg.dc, 4); // HALFTONE
    we_win32::SetBrushOrgEx(bg.dc, 0, 0, std::ptr::null_mut());
    we_win32::StretchBlt(
        bg.dc,
        bx - origin.x + d.left,
        by - origin.y + d.top,
        d.right - d.left,
        d.bottom - d.top,
        src.dc,
        s.left,
        s.top,
        s.right - s.left,
        s.bottom - s.top,
        0x00CC0020, // SRCCOPY
    );

    // The UI capture is "UI premultiplied over black" without alpha. Estimate coverage
    // from brightness (the theme's dark panels/gradients are near black), so fully
    // transparent areas show the scene and dark translucent layers still darken it.
    let u = ui.pixels();
    let b = bg.pixels();
    let mut out = vec![0u8; u.len()];
    for i in (0..u.len()).step_by(4) {
        let m = u[i].max(u[i + 1]).max(u[i + 2]) as u32;
        let inv = 255 - (m * 255 / 24).min(255);
        for c in 0..3 {
            out[i + c] = (u[i + c] as u32 + b[i + c] as u32 * inv / 255).min(255) as u8;
        }
        out[i + 3] = 255;
    }
    Some((out, cw, ch))
}

type Preview = std::sync::Arc<(Vec<u8>, i32, i32)>;

/// Cached preview, or a synchronous compose if there is none yet.
#[cfg(target_os = "windows")]
unsafe fn cached_taskbar_preview(app_hwnd: we_win32::HWND) -> Option<Preview> {
    if let Some(p) = TASKBAR_PREVIEW.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return Some(p);
    }
    let p: Preview = std::sync::Arc::new(compose_taskbar_preview(app_hwnd)?);
    *TASKBAR_PREVIEW.lock().unwrap_or_else(|e| e.into_inner()) = Some(p.clone());
    Some(p)
}

/// Recompose the preview now (synchronously) if the scene can be captured.
#[cfg(target_os = "windows")]
fn refresh_taskbar_preview_cache(app_hwnd: we_win32::HWND) {
    if get_we_window_hwnd().is_none() {
        return;
    }
    if let Some(p) = unsafe { compose_taskbar_preview(app_hwnd) } {
        *TASKBAR_PREVIEW.lock().unwrap_or_else(|e| e.into_inner()) = Some(std::sync::Arc::new(p));
    }
}

/// Last thumbnail size DWM asked for, so a background refresh can resend it.
static LAST_THUMB_SIZE: Mutex<Option<(i32, i32)>> = Mutex::new(None);
static PREVIEW_REFRESHING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Recompose off the UI thread (composing a full-size frame is slow, and DWM shows a
/// spinner until the thumbnail arrives), then push the fresh thumbnail if DWM has
/// already asked for one.
#[cfg(target_os = "windows")]
fn spawn_taskbar_preview_refresh(app_hwnd: we_win32::HWND) {
    if PREVIEW_REFRESHING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let app = app_hwnd as usize;
    std::thread::spawn(move || {
        let app_hwnd = app as we_win32::HWND;
        refresh_taskbar_preview_cache(app_hwnd);
        let size = *LAST_THUMB_SIZE.lock().unwrap_or_else(|e| e.into_inner());
        if let (Some((w, h)), Some(p)) = (size, TASKBAR_PREVIEW.lock().unwrap_or_else(|e| e.into_inner()).clone()) {
            if get_we_window_hwnd().is_some() {
                unsafe { set_iconic_thumbnail(app_hwnd, &p, w, h) };
            }
        }
        PREVIEW_REFRESHING.store(false, std::sync::atomic::Ordering::SeqCst);
    });
}

#[cfg(target_os = "windows")]
unsafe fn set_iconic_thumbnail(app_hwnd: we_win32::HWND, p: &Preview, max_w: i32, max_h: i32) {
    let (px, w, h) = (&p.0, p.1, p.2);
    let k = (max_w as f32 / w as f32).min(max_h as f32 / h as f32).min(1.0);
    let (tw, th) = (((w as f32 * k) as i32).max(1), ((h as f32 * k) as i32).max(1));
    let (Some(src), Some(dst)) = (Dib::from_pixels(px, w, h), Dib::new(tw, th)) else { return };
    we_win32::SetStretchBltMode(dst.dc, 4);
    we_win32::SetBrushOrgEx(dst.dc, 0, 0, std::ptr::null_mut());
    we_win32::StretchBlt(dst.dc, 0, 0, tw, th, src.dc, 0, 0, w, h, 0x00CC0020);
    // GDI does not keep alpha; DWM expects opaque premultiplied pixels.
    for a in dst.pixels().iter_mut().skip(3).step_by(4) {
        *a = 255;
    }
    let bmp = dst.into_bitmap();
    let _ = we_win32::DwmSetIconicThumbnail(app_hwnd, bmp, 0);
    we_win32::DeleteObject(bmp);
}

/// WM_DWMSENDICONICTHUMBNAIL: answer immediately from the cache, refresh in background.
#[cfg(target_os = "windows")]
unsafe fn send_iconic_thumbnail(app_hwnd: we_win32::HWND, max_w: i32, max_h: i32) {
    *LAST_THUMB_SIZE.lock().unwrap_or_else(|e| e.into_inner()) = Some((max_w, max_h));
    if let Some(p) = cached_taskbar_preview(app_hwnd) {
        set_iconic_thumbnail(app_hwnd, &p, max_w, max_h);
    }
    if we_win32::IsIconic(app_hwnd) == 0 {
        spawn_taskbar_preview_refresh(app_hwnd);
    }
}

#[cfg(target_os = "windows")]
unsafe fn send_iconic_live_preview(app_hwnd: we_win32::HWND) {
    let Some(p) = cached_taskbar_preview(app_hwnd) else { return };
    let Some(dib) = Dib::from_pixels(&p.0, p.1, p.2) else { return };
    let bmp = dib.into_bitmap();
    let _ = we_win32::DwmSetIconicLivePreviewBitmap(app_hwnd, bmp, std::ptr::null(), 0);
    we_win32::DeleteObject(bmp);
}

/// Turns the custom taskbar preview on (scene active) or back to Windows' default.
#[cfg(target_os = "windows")]
fn set_taskbar_preview_override(app_hwnd: we_win32::HWND, on: bool) {
    if app_hwnd.is_null() {
        return;
    }
    *PREVIEW_APP_HWND.lock().unwrap_or_else(|e| e.into_inner()) = Some(app_hwnd as isize);
    let v: i32 = on as i32;
    unsafe {
        for attr in [we_win32::DWMWA_FORCE_ICONIC_REPRESENTATION, we_win32::DWMWA_HAS_ICONIC_BITMAP] {
            let _ = we_win32::DwmSetWindowAttribute(
                app_hwnd,
                attr,
                &v as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<i32>() as u32,
            );
        }
        if on {
            let _ = we_win32::DwmInvalidateIconicBitmaps(app_hwnd);
        }
    }
    if !on {
        *TASKBAR_PREVIEW.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *LAST_THUMB_SIZE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// True once the current scene is actually drawing behind the launcher (not just a
/// freshly created, still black window). Used to fade the freeze-frame out only when
/// there is something to reveal. Same capture trick as `capture_we_snapshot`: the
/// launcher is excluded from capture for a moment, which is not visible on screen.
#[tauri::command]
fn is_we_scene_rendering(window: tauri::WebviewWindow) -> bool {
    #[cfg(target_os = "windows")]
    unsafe {
        let Ok(h) = window.hwnd() else { return false };
        let app_hwnd = h.0 as we_win32::HWND;
        let underlay_visible = get_underlay_hwnd()
            .map(|u| we_win32::IsWindowVisible(u as we_win32::HWND) != 0)
            .unwrap_or(false);
        if app_hwnd.is_null() || get_we_window_hwnd().is_none() || !underlay_visible {
            return false;
        }
        let (x, y, w, h) = we_win32::get_visible_window_rect(app_hwnd);
        let (w, h) = (w as i32, h as i32);
        const SW: i32 = 32;
        const SH: i32 = 18;

        let hdc_screen = we_win32::GetDC(std::ptr::null_mut());
        if hdc_screen.is_null() {
            return false;
        }
        let hdc_mem = we_win32::CreateCompatibleDC(hdc_screen);
        let hbm = we_win32::CreateCompatibleBitmap(hdc_screen, SW, SH);
        let mut lit = 0;
        if !hdc_mem.is_null() && !hbm.is_null() {
            let old = we_win32::SelectObject(hdc_mem, hbm);
            if we_win32::SetWindowDisplayAffinity(app_hwnd, 0x11) != 0 {
                we_win32::DwmFlush();
                we_win32::SetStretchBltMode(hdc_mem, 4);
                we_win32::StretchBlt(hdc_mem, 0, 0, SW, SH, hdc_screen, x, y, w, h, 0x00CC0020);
                we_win32::SetWindowDisplayAffinity(app_hwnd, 0);
                for sy in 0..SH {
                    for sx in 0..SW {
                        let c = we_win32::GetPixel(hdc_mem, sx, sy);
                        if c != 0xFFFF_FFFF && (c & 0xFF) + ((c >> 8) & 0xFF) + ((c >> 16) & 0xFF) > 30 {
                            lit += 1;
                        }
                    }
                }
            }
            we_win32::SelectObject(hdc_mem, old);
        }
        if !hbm.is_null() {
            we_win32::DeleteObject(hbm);
        }
        if !hdc_mem.is_null() {
            we_win32::DeleteDC(hdc_mem);
        }
        we_win32::ReleaseDC(std::ptr::null_mut(), hdc_screen);
        // A few lit samples are enough; very dark scenes still have highlights.
        return lit >= 4;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = window;
        true
    }
}

/// Pausing the launcher's Wallpaper Engine scene while the launcher is not in use.
///
/// WE has no working pause for -playInWindow windows: `-control pause` (with or without
/// `-location`) keeps them animating, and hiding/minimizing/cloaking the window doesn't
/// stop the rendering either. What works is suspending the few WE threads that render
/// this scene: among the threads WE creates when the scene is opened (~100), only 2-3
/// actually use CPU. With those suspended WE's CPU and GPU usage for the scene drop to
/// zero, WE keeps answering commands, and a close requested meanwhile completes as soon
/// as the threads are resumed.
#[cfg(target_os = "windows")]
mod we_freeze {
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    type HANDLE = *mut std::ffi::c_void;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct THREADENTRY32 {
        dwSize: u32,
        cntUsage: u32,
        th32ThreadID: u32,
        th32OwnerProcessID: u32,
        tpBasePri: i32,
        tpDeltaPri: i32,
        dwFlags: u32,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct PROCESSENTRY32W {
        dwSize: u32,
        cntUsage: u32,
        th32ProcessID: u32,
        th32DefaultHeapID: usize,
        th32ModuleID: u32,
        cntThreads: u32,
        th32ParentProcessID: u32,
        pcPriClassBase: i32,
        dwFlags: u32,
        szExeFile: [u16; 260],
    }

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct FILETIME {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> HANDLE;
        fn Thread32First(snapshot: HANDLE, entry: *mut THREADENTRY32) -> i32;
        fn Thread32Next(snapshot: HANDLE, entry: *mut THREADENTRY32) -> i32;
        fn Process32FirstW(snapshot: HANDLE, entry: *mut PROCESSENTRY32W) -> i32;
        fn Process32NextW(snapshot: HANDLE, entry: *mut PROCESSENTRY32W) -> i32;
        fn OpenThread(access: u32, inherit: i32, tid: u32) -> HANDLE;
        fn SuspendThread(thread: HANDLE) -> u32;
        fn ResumeThread(thread: HANDLE) -> u32;
        fn GetThreadTimes(
            thread: HANDLE,
            creation: *mut FILETIME,
            exit: *mut FILETIME,
            kernel: *mut FILETIME,
            user: *mut FILETIME,
        ) -> i32;
        fn CloseHandle(handle: HANDLE) -> i32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetWindowThreadProcessId(hwnd: HANDLE, pid: *mut u32) -> u32;
    }

    const TH32CS_SNAPPROCESS: u32 = 0x2;
    const TH32CS_SNAPTHREAD: u32 = 0x4;
    const THREAD_SUSPEND_RESUME: u32 = 0x0002;
    const THREAD_QUERY_LIMITED_INFORMATION: u32 = 0x0800;
    const INVALID_HANDLE_VALUE: isize = -1;
    /// A thread counts as rendering if it used >= 2 ms of CPU in a 700 ms window.
    const BUSY_100NS: u64 = 20_000;

    struct Baseline {
        pid: u32,
        threads: HashSet<u32>,
    }

    /// WE's threads right before the scene was opened.
    static BASELINE: Mutex<Option<Baseline>> = Mutex::new(None);
    /// Threads we suspended: (thread id, handle).
    static FROZEN: Mutex<Vec<(u32, usize)>> = Mutex::new(Vec::new());
    /// Cleared by thaw(): a freeze still in progress must give up and undo itself.
    static WANT_FROZEN: AtomicBool = AtomicBool::new(false);
    /// Set while the launcher opens/closes the scene on its UI thread. Freezing then
    /// would suspend threads the UI thread is synchronously waiting on (it configures
    /// the new window right after creating it) → deadlock.
    static SCENE_BUSY: AtomicBool = AtomicBool::new(false);

    /// Marks a scene open/close in progress for as long as the guard lives; also thaws.
    pub struct SceneBusyGuard;
    impl Drop for SceneBusyGuard {
        fn drop(&mut self) {
            SCENE_BUSY.store(false, Ordering::SeqCst);
        }
    }
    pub fn begin_scene_operation() -> SceneBusyGuard {
        SCENE_BUSY.store(true, Ordering::SeqCst);
        thaw();
        SceneBusyGuard
    }

    fn may_freeze() -> bool {
        WANT_FROZEN.load(Ordering::SeqCst) && !SCENE_BUSY.load(Ordering::SeqCst)
    }

    fn snapshot(flags: u32) -> Option<HANDLE> {
        let snap = unsafe { CreateToolhelp32Snapshot(flags, 0) };
        if snap.is_null() || snap as isize == INVALID_HANDLE_VALUE {
            None
        } else {
            Some(snap)
        }
    }

    fn we_pid() -> Option<u32> {
        let snap = snapshot(TH32CS_SNAPPROCESS)?;
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = None;
        unsafe {
            let mut ok = Process32FirstW(snap, &mut entry);
            while ok != 0 {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(260);
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
                if name == "wallpaper64.exe" || name == "wallpaper32.exe" {
                    found = Some(entry.th32ProcessID);
                    break;
                }
                ok = Process32NextW(snap, &mut entry);
            }
            CloseHandle(snap);
        }
        found
    }

    fn threads_of(pid: u32) -> HashSet<u32> {
        let mut out = HashSet::new();
        let Some(snap) = snapshot(TH32CS_SNAPTHREAD) else { return out };
        let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        unsafe {
            let mut ok = Thread32First(snap, &mut entry);
            while ok != 0 {
                if entry.th32OwnerProcessID == pid {
                    out.insert(entry.th32ThreadID);
                }
                ok = Thread32Next(snap, &mut entry);
            }
            CloseHandle(snap);
        }
        out
    }

    fn cpu_time(thread: HANDLE) -> u64 {
        let (mut c, mut e, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        if unsafe { GetThreadTimes(thread, &mut c, &mut e, &mut k, &mut u) } == 0 {
            return 0;
        }
        let t = |f: FILETIME| ((f.high as u64) << 32) | f.low as u64;
        t(k) + t(u)
    }

    fn marker_path() -> std::path::PathBuf {
        std::env::temp_dir().join("ika_launcher_we_frozen.txt")
    }

    fn write_marker(pid: u32, tids: &[u32]) {
        let list: Vec<String> = tids.iter().map(|t| t.to_string()).collect();
        let _ = std::fs::write(marker_path(), format!("{}\n{}", pid, list.join(",")));
    }

    /// Call right before WE is asked to open the scene.
    pub fn record_baseline() {
        let baseline = we_pid().map(|pid| Baseline { pid, threads: threads_of(pid) });
        *BASELINE.lock().unwrap_or_else(|e| e.into_inner()) = baseline;
    }

    pub fn clear_baseline() {
        *BASELINE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// Suspends the scene's rendering threads. Returns false (and leaves nothing
    /// suspended) if they can't be identified, if rendering doesn't stop, or if thaw()
    /// was called meanwhile.
    /// `window` is the scene's window: its thread is never suspended, because the launcher
    /// calls ShowWindow/SetWindowPos on it, which would block on a suspended thread.
    pub fn freeze(window: isize) -> bool {
        if !FROZEN.lock().unwrap_or_else(|e| e.into_inner()).is_empty() {
            return true;
        }
        WANT_FROZEN.store(true, Ordering::SeqCst);
        if !may_freeze() {
            return false;
        }
        let Some((pid, base)) = BASELINE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|b| (b.pid, b.threads.clone()))
        else {
            return false;
        };
        let window_tid = unsafe { GetWindowThreadProcessId(window as HANDLE, std::ptr::null_mut()) };
        let scene: Vec<(u32, HANDLE)> = threads_of(pid)
            .into_iter()
            .filter(|tid| !base.contains(tid) && *tid != window_tid)
            .filter_map(|tid| {
                let h = unsafe { OpenThread(THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION, 0, tid) };
                (!h.is_null()).then_some((tid, h))
            })
            .collect();

        let mut frozen: Vec<usize> = Vec::new(); // indexes into `scene`
        let mut idle = false;
        for _round in 0..3 {
            if !may_freeze() {
                break;
            }
            let before: Vec<u64> = scene.iter().map(|(_, h)| cpu_time(*h)).collect();
            std::thread::sleep(std::time::Duration::from_millis(700));
            let busy: Vec<usize> = (0..scene.len())
                .filter(|i| !frozen.contains(i) && cpu_time(scene[*i].1).saturating_sub(before[*i]) >= BUSY_100NS)
                .collect();
            if busy.is_empty() {
                idle = !frozen.is_empty();
                break;
            }
            if !may_freeze() {
                break;
            }
            for i in busy {
                if unsafe { SuspendThread(scene[i].1) } != u32::MAX {
                    frozen.push(i);
                }
            }
            let tids: Vec<u32> = frozen.iter().map(|&i| scene[i].0).collect();
            write_marker(pid, &tids);
        }

        let keep = idle && may_freeze();
        let mut stored = FROZEN.lock().unwrap_or_else(|e| e.into_inner());
        for (i, (tid, h)) in scene.iter().enumerate() {
            if keep && frozen.contains(&i) {
                stored.push((*tid, *h as usize));
            } else {
                unsafe {
                    if frozen.contains(&i) {
                        ResumeThread(*h);
                    }
                    CloseHandle(*h);
                }
            }
        }
        if !keep {
            let _ = std::fs::remove_file(marker_path());
        }
        keep
    }

    pub fn is_frozen() -> bool {
        !FROZEN.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
    }

    /// Resumes everything freeze() suspended (and cancels a freeze in progress).
    pub fn thaw() {
        WANT_FROZEN.store(false, Ordering::SeqCst);
        let mut stored = FROZEN.lock().unwrap_or_else(|e| e.into_inner());
        for (_, h) in stored.drain(..) {
            unsafe {
                ResumeThread(h as HANDLE);
                CloseHandle(h as HANDLE);
            }
        }
        let _ = std::fs::remove_file(marker_path());
    }

    /// If a previous launcher session ended while the scene was frozen (crash, killed),
    /// resume those threads so WE isn't left with a stuck window.
    pub fn recover_after_crash() {
        let Ok(text) = std::fs::read_to_string(marker_path()) else { return };
        let mut lines = text.lines();
        let pid: u32 = lines.next().and_then(|l| l.trim().parse().ok()).unwrap_or(0);
        if pid != 0 && we_pid() == Some(pid) {
            let alive = threads_of(pid);
            for tid in lines.next().unwrap_or("").split(',').filter_map(|t| t.trim().parse::<u32>().ok()) {
                if !alive.contains(&tid) {
                    continue;
                }
                unsafe {
                    let h = OpenThread(THREAD_SUSPEND_RESUME, 0, tid);
                    if !h.is_null() {
                        ResumeThread(h);
                        CloseHandle(h);
                    }
                }
            }
        }
        let _ = std::fs::remove_file(marker_path());
    }
}

/// Must run before any ShowWindow/SetWindowPos/PrintWindow on the WE window: its thread
/// is not suspended, but it synchronously waits on the (suspended) render threads, so
/// those calls would block the launcher's UI thread forever.
fn thaw_we_scene_before_window_ops() {
    #[cfg(target_os = "windows")]
    we_freeze::thaw();
}

/// Pauses the launcher's WE scene (see `we_freeze`). Runs off the UI thread: it samples
/// thread CPU usage for about a second.
#[tauri::command(async)]
fn freeze_we_scene() -> bool {
    #[cfg(target_os = "windows")]
    {
        let Some(window) = get_we_window_hwnd() else { return false };
        return we_freeze::freeze(window);
    }
    #[cfg(not(target_os = "windows"))]
    false
}

/// Async (worker thread) on purpose: it must work even if the UI thread is stuck on a
/// call to the frozen scene's window — resuming the threads is what unblocks it.
#[tauri::command(async)]
fn thaw_we_scene() {
    #[cfg(target_os = "windows")]
    we_freeze::thaw();
}

/// Async on purpose: a sync command runs on the window's UI thread, and switching a
/// scene waits for Wallpaper Engine for several seconds (tasklist, closing the old
/// window, waiting for the new one). Doing that on the UI thread froze mouse input
/// while WebView2 kept animating. The waits now run on a worker thread and only the
/// Win32 window setup is sent back to the UI thread (see `play_we_wallpaper_blocking`).
#[tauri::command]
async fn play_we_wallpaper(
    window: tauri::WebviewWindow,
    project_path: String,
    properties: Option<serde_json::Value>,
) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || play_we_wallpaper_blocking(window, project_path, properties))
        .await
        .map_err(|e| e.to_string())?
}

fn play_we_wallpaper_blocking(
    window: tauri::WebviewWindow,
    project_path: String,
    properties: Option<serde_json::Value>,
) -> Result<bool, String> {
    let we_exe = find_wallpaper_engine_path()
        .ok_or_else(|| "Wallpaper Engine no se encuentra instalado en las rutas estándar de Steam.".to_string())?;

    let target_file = resolve_we_target_file(&project_path);

    if !target_file.exists() {
        return Err(format!("El archivo de wallpaper no existe: {}", target_file.display()));
    }

    // Invalidate follow-up threads of the previous scene before touching anything.
    let gen = bump_we_generation();

    // A frozen previous scene must run again before WE is asked to close it, and no
    // freeze may start while this scene is being opened (see SCENE_BUSY).
    #[cfg(target_os = "windows")]
    let _scene_busy = we_freeze::begin_scene_operation();

    // Who has focus right now, before WE opens a new window (see step 11 below).
    #[cfg(target_os = "windows")]
    let prev_foreground = unsafe { we_win32::GetForegroundWindow() };

    // Hide the previous window, drop its thumbnail, then close it in WE and wait until
    // it is completely gone: only one scene may exist at a time.
    #[cfg(target_os = "windows")]
    let old_hwnd = take_we_window_hwnd();
    #[cfg(target_os = "windows")]
    {
        // The frontend has just put the freeze-frame on screen, but WebView2 presents it
        // through its own compositor a few frames later. Let DWM compose a couple of
        // frames before the old scene disappears, or the dark base shows through.
        if old_hwnd.is_some() {
            for _ in 0..3 {
                unsafe {
                    we_win32::DwmFlush();
                }
            }
        }
        if let Some(thumb_val) = take_we_thumbnail_id() {
            unsafe {
                let _ = we_win32::DwmUnregisterThumbnail(thumb_val as we_win32::HTHUMBNAIL);
            }
        }
        if let Some(h) = old_hwnd {
            unsafe {
                we_win32::ShowWindow(h as we_win32::HWND, we_win32::SW_HIDE);
            }
        }
        close_we_window_and_wait(&we_exe, old_hwnd);
    }

    *CURRENT_WE_TITLE.lock().unwrap_or_else(|e| e.into_inner()) = WE_WINDOW_TITLE.to_string();
    *CURRENT_WE_PATH.lock().unwrap_or_else(|e| e.into_inner()) = project_path.clone();

    let clean_props = properties.as_ref().map(clean_we_properties).unwrap_or_default();

    // Ensure Wallpaper Engine daemon is running first so it can process commands
    ensure_wallpaper_engine_running(&we_exe);

    #[cfg(target_os = "windows")]
    let app_hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as we_win32::HWND;
    #[cfg(target_os = "windows")]
    let geometry = we_window_geometry(app_hwnd);
    #[cfg(not(target_os = "windows"))]
    let geometry = (60, 60, 1120, 660);

    // WE's threads before the scene exists: the ones that appear afterwards are this
    // scene's, which is what freeze_we_scene suspends.
    #[cfg(target_os = "windows")]
    we_freeze::record_baseline();

    spawn_we_window(&we_exe, &target_file, WE_WINDOW_TITLE, geometry)?;

    #[cfg(target_os = "windows")]
    {
        let found_hwnd = wait_for_we_window(WE_WINDOW_TITLE, old_hwnd);

        if !found_hwnd.is_null() {
            set_we_window_hwnd(Some(found_hwnd as isize));

            // The underlay window is created here (update_we_geometry_internal) and a
            // window belongs to the thread that creates it, so this part must run on the
            // UI thread, which pumps its messages. HWNDs travel as integers (not Send).
            let fh = found_hwnd as usize;
            let ah = app_hwnd as usize;
            let pf = prev_foreground as usize;
            let (tx, rx) = std::sync::mpsc::channel();
            window
                .run_on_main_thread(move || {
                    unsafe { finish_we_window_setup(fh as _, ah as _, pf as _) };
                    let _ = tx.send(());
                })
                .map_err(|e| e.to_string())?;
            let _ = rx.recv();

            // 12. Schedule follow-up updates so when Wallpaper Engine finishes loading the new scene,
            // thumbnail source dimensions and properties are perfectly synchronized
            std::thread::spawn(move || {
                for delay_ms in [250, 600, 1200] {
                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                    if !is_current_we_generation(gen) {
                        return;
                    }
                    if delay_ms == 1200 {
                        // Scene is loaded by now: prepare the taskbar preview ahead of
                        // time so hovering the taskbar never has to wait for it.
                        spawn_taskbar_preview_refresh(ah as we_win32::HWND);
                    }
                    let (s, px, py) = get_we_transform();
                    update_we_geometry_internal(
                        fh as we_win32::HWND,
                        ah as we_win32::HWND,
                        s,
                        px,
                        py,
                    );
                }
            });

            // The scene ignores properties sent before it finishes loading, so resend the
            // saved ones a few times (it's idempotent) while this window is still current.
            if !clean_props.is_empty() {
                let we_exe_clone = we_exe.clone();
                std::thread::spawn(move || {
                    for delay_ms in [300, 900, 2000, 3500] {
                        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                        if !is_current_we_generation(gen) {
                            return;
                        }
                        send_we_properties(&we_exe_clone, WE_WINDOW_TITLE, &clean_props);
                    }
                });
            }
            return Ok(true);
        }
    }

    Ok(false)
}

/// UI-thread half of `play_we_wallpaper`: configures the new WE window, creates the
/// underlay/thumbnail and restores focus.
#[cfg(target_os = "windows")]
unsafe fn finish_we_window_setup(
    found_hwnd: we_win32::HWND,
    app_hwnd: we_win32::HWND,
    prev_foreground: we_win32::HWND,
) {
    prepare_we_window(found_hwnd);

    // Apply current scale & pan transform with hardware DWM thumbnail scaling
    let (scale, pan_x, pan_y) = get_we_transform();
    update_we_geometry_internal(found_hwnd, app_hwnd, scale, pan_x, pan_y);

    // Taskbar thumbnail: show the scene instead of the transparent launcher.
    set_taskbar_preview_override(app_hwnd, true);

    // 10. DeleteTab again after show to guarantee shell removal
    we_win32::delete_taskbar_tab(found_hwnd);
    if let Some(u_val) = get_underlay_hwnd() {
        we_win32::delete_taskbar_tab(u_val as we_win32::HWND);
    }

    // 11. Hand focus back to whoever had it before the switch. Only pull the
    // launcher to the front if the user was in it; if they were in another
    // app (playlist rotating in the background), leave that app in front
    // and undo any focus change caused by WE opening its window.
    if prev_foreground == app_hwnd {
        we_win32::SetWindowPos(
            app_hwnd,
            0 as we_win32::HWND, // HWND_TOP
            0, 0, 0, 0,
            we_win32::SWP_NOMOVE | we_win32::SWP_NOSIZE | we_win32::SWP_SHOWWINDOW,
        );
        we_win32::BringWindowToTop(app_hwnd);
        we_win32::SetForegroundWindow(app_hwnd);
        we_win32::SetActiveWindow(app_hwnd);
    } else if !prev_foreground.is_null()
        && we_win32::IsWindow(prev_foreground) != 0
        && we_win32::GetForegroundWindow() != prev_foreground
    {
        we_win32::SetForegroundWindow(prev_foreground);
    }
}

#[tauri::command]
fn stop_we_wallpaper() -> Result<(), String> {
    bump_we_generation();
    #[cfg(target_os = "windows")]
    let _scene_busy = we_freeze::begin_scene_operation();
    #[cfg(target_os = "windows")]
    we_freeze::clear_baseline();
    #[cfg(target_os = "windows")]
    {
        let app_hwnd = PREVIEW_APP_HWND.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(h) = app_hwnd {
            set_taskbar_preview_override(h as we_win32::HWND, false);
        }
        if let Some(thumb_val) = take_we_thumbnail_id() {
            unsafe {
                let _ = we_win32::DwmUnregisterThumbnail(thumb_val as we_win32::HTHUMBNAIL);
            }
        }
        if let Some(underlay_val) = take_underlay_hwnd() {
            unsafe {
                we_win32::ShowWindow(underlay_val as we_win32::HWND, we_win32::SW_HIDE);
                we_win32::DestroyWindow(underlay_val as we_win32::HWND);
            }
        }
        if let Some(hwnd_val) = take_we_window_hwnd() {
            unsafe {
                we_win32::ShowWindow(hwnd_val as we_win32::HWND, we_win32::SW_HIDE);
            }
        }
        *LAST_WE_GEOM.lock().unwrap_or_else(|e| e.into_inner()) = (0, 0, 0, 0);
        *LAST_WE_RGN.lock().unwrap_or_else(|e| e.into_inner()) = (0, 0, 0, 0);
    }

    if let Some(we_exe) = find_wallpaper_engine_path() {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            let _ = std::process::Command::new(&we_exe)
                .args(["-control", "closeWallpaper", "-location", WE_WINDOW_TITLE])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = std::process::Command::new(&we_exe)
                .args(["-control", "closeWallpaper", "-location", WE_WINDOW_TITLE])
                .spawn();
        }
    }

    *CURRENT_WE_TITLE.lock().unwrap_or_else(|e| e.into_inner()) = String::new();
    *CURRENT_WE_PATH.lock().unwrap_or_else(|e| e.into_inner()) = String::new();
    Ok(())
}


#[tauri::command]
fn sync_we_window_rect(window: tauri::WebviewWindow, visible: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(hwnd_val) = get_we_window_hwnd() {
            thaw_we_scene_before_window_ops();
            let hwnd = hwnd_val as we_win32::HWND;
            unsafe {
                let app_hwnd = window.hwnd().map(|h| h.0 as we_win32::HWND).unwrap_or(std::ptr::null_mut());
                let is_min = if !app_hwnd.is_null() {
                    let (win_x, win_y, win_w, win_h) = we_win32::get_visible_window_rect(app_hwnd);
                    we_win32::IsIconic(app_hwnd) != 0
                        || win_x < -10000
                        || win_y < -10000
                        || win_w < 50
                        || win_h < 50
                } else {
                    true
                };

                if visible && !is_min {
                    if !app_hwnd.is_null() {
                        let (scale, pan_x, pan_y) = get_we_transform();
                        update_we_geometry_internal(hwnd, app_hwnd, scale, pan_x, pan_y);
                    }
                } else {
                    if let Some(u_val) = get_underlay_hwnd() {
                        we_win32::ShowWindow(u_val as we_win32::HWND, we_win32::SW_HIDE);
                    }
                    we_win32::ShowWindow(hwnd, we_win32::SW_HIDE);
                    *LAST_WE_GEOM.lock().unwrap_or_else(|e| e.into_inner()) = (0, 0, 0, 0);
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn update_we_transform(window: tauri::WebviewWindow, scale: f64, pan_x: f64, pan_y: f64) -> Result<(), String> {
    set_we_transform(scale as f32, pan_x as f32, pan_y as f32);
    #[cfg(target_os = "windows")]
    {
        if let Some(hwnd_val) = get_we_window_hwnd() {
            let app_hwnd = window.hwnd().map(|h| h.0 as we_win32::HWND).unwrap_or(std::ptr::null_mut());
            if !app_hwnd.is_null() {
                update_we_geometry_internal(
                    hwnd_val as we_win32::HWND,
                    app_hwnd,
                    scale as f32,
                    pan_x as f32,
                    pan_y as f32,
                );
            }
        }
    }
    Ok(())
}

#[tauri::command(async)]
fn apply_we_properties(
    properties: serde_json::Value,
    project_path: Option<String>,
) -> Result<(), String> {
    let we_exe = find_wallpaper_engine_path()
        .ok_or_else(|| "Wallpaper Engine no se encuentra instalado.".to_string())?;

    let clean_obj = clean_we_properties(&properties);
    if clean_obj.is_empty() {
        return Ok(());
    }

    let cur_path = CURRENT_WE_PATH.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if cur_path.is_empty() {
        return Ok(());
    }
    // Options edited for a wallpaper that isn't the one currently playing are just saved by the UI.
    if let Some(p) = project_path.filter(|p| !p.trim().is_empty()) {
        if !same_we_project(&p, &cur_path) {
            return Ok(());
        }
    }

    let cur_title = CURRENT_WE_TITLE.lock().unwrap_or_else(|e| e.into_inner()).clone();
    send_we_properties(&we_exe, &cur_title, &clean_obj);
    Ok(())
}

#[tauri::command(async)]
fn get_we_wallpaper_properties(project_path: String) -> Result<Option<serde_json::Value>, String> {
    let path = std::path::Path::new(&project_path);
    let project_json = if path.is_file() {
        if path.file_name().and_then(|s| s.to_str()) == Some("project.json") {
            path.to_path_buf()
        } else {
            path.parent().map(|p| p.join("project.json")).unwrap_or_else(|| path.to_path_buf())
        }
    } else {
        path.join("project.json")
    };

    if !project_json.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&project_json).map_err(|e| e.to_string())?;
    let val: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let props = val.get("general").and_then(|g| g.get("properties")).cloned();
    Ok(props)
}

fn get_block_textures_dir() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let p = parent.join("minecraft_block_textures");
            if p.exists() {
                return p;
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let p = cwd.join("minecraft_block_textures");
        if p.exists() {
            return p;
        }
    }
    let fallback = std::path::PathBuf::from(r"C:\dev\Comparer\minecraft_block_textures");
    if fallback.exists() {
        return fallback;
    }
    std::env::current_dir().unwrap_or_default().join("minecraft_block_textures")
}

#[tauri::command(async)]
fn get_minecraft_block_textures() -> Vec<String> {
    let dir = get_block_textures_dir();
    let mut textures = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext.eq_ignore_ascii_case("png") {
                        if let Some(p_str) = path.to_str() {
                            textures.push(p_str.to_string());
                        }
                    }
                }
            }
        }
    }
    textures.sort();
    textures
}

#[tauri::command(async)]
fn open_block_textures_folder() -> Result<(), String> {
    let dir = get_block_textures_dir();
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
    }
    open::that(&dir).map_err(|e| e.to_string())
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct InstanceLogStatus {
    pub state: String,         // "idle", "loading", "running", "crashed"
    pub status_headline: String,
    pub status_sub: String,
    pub has_crashed: bool,
    pub is_running: bool,
    pub last_lines: Vec<String>,
}

#[tauri::command(async)]
fn get_instance_log_status(instance_path: String) -> InstanceLogStatus {
    let p = std::path::PathBuf::from(&instance_path);
    let log_candidates = [
        p.join("minecraft").join("logs").join("latest.log"),
        p.join(".minecraft").join("logs").join("latest.log"),
        p.join("logs").join("latest.log"),
    ];

    let mut found_log = None;
    for c in &log_candidates {
        if c.exists() {
            found_log = Some(c.clone());
            break;
        }
    }

    let mut lines = Vec::new();
    let mut modified_secs_ago = 999999;
    let mut has_crashed = false;
    let mut is_running = false;

    if let Some(log_path) = found_log {
        if let Ok(meta) = std::fs::metadata(&log_path) {
            if let Ok(mod_time) = meta.modified() {
                if let Ok(dur) = std::time::SystemTime::now().duration_since(mod_time) {
                    modified_secs_ago = dur.as_secs();
                }
            }
        }

        if let Ok(file) = std::fs::File::open(&log_path) {
            let reader = BufReader::new(file);
            let all_lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
            let total = all_lines.len();
            let start = if total > 30 { total - 30 } else { 0 };
            let tail = &all_lines[start..];

            for l in tail {
                let lower = l.to_lowercase();
                if lower.contains("crash report saved")
                    || lower.contains("this crash report has been saved")
                    || lower.contains("the game crashed whilst")
                    || lower.contains("encountered an unexpected exception")
                    || lower.contains("exception_access_violation")
                {
                    has_crashed = true;
                }
                if lower.contains("setting user:")
                    || lower.contains("backend library: lwjgl")
                    || lower.contains("openal initialized")
                    || lower.contains("sound engine started")
                    || lower.contains("minecraft initialization complete")
                    || lower.contains("textures-atlas")
                {
                    is_running = true;
                }
            }

            let start_last = if total > 8 { total - 8 } else { 0 };
            lines = all_lines[start_last..].to_vec();
        }
    }

    // Check crash reports directory
    let crash_candidates = [
        p.join("minecraft").join("crash-reports"),
        p.join(".minecraft").join("crash-reports"),
        p.join("crash-reports"),
    ];
    for c_dir in &crash_candidates {
        if let Ok(entries) = std::fs::read_dir(c_dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(m) = meta.modified() {
                        if let Ok(dur) = std::time::SystemTime::now().duration_since(m) {
                            if dur.as_secs() < 300 {
                                has_crashed = true;
                            }
                        }
                    }
                }
            }
        }
    }

    let (state, headline, sub) = if has_crashed {
        (
            "crashed".to_string(),
            "¡Crasheo Detectado!".to_string(),
            "El juego se cerró con un error. Revisa la consola.".to_string(),
        )
    } else if is_running && modified_secs_ago < 300 {
        (
            "running".to_string(),
            "¡Juego lanzado con éxito!".to_string(),
            "Minecraft ejecutándose correctamente • Sin crasheos".to_string(),
        )
    } else if modified_secs_ago < 45 {
        (
            "loading".to_string(),
            "Cargando juego y mods...".to_string(),
            "Inicializando dependencias de Minecraft...".to_string(),
        )
    } else {
        (
            "idle".to_string(),
            "Listo para jugar".to_string(),
            "Presiona JUGAR para iniciar Minecraft".to_string(),
        )
    };

    InstanceLogStatus {
        state,
        status_headline: headline,
        status_sub: sub,
        has_crashed,
        is_running,
        last_lines: lines,
    }
}

#[derive(serde::Serialize, Clone, Debug)]
pub struct JavaInstallation {
    pub path: String,
    pub version: String,
    pub major: u32,
    /// Display name, e.g. "Eclipse Adoptium"
    pub vendor: String,
    /// Stable id: mojang | adoptium | zulu | semeru | microsoft | oracle | other
    pub vendor_id: String,
}

/// Classifies a Java home by its path first (Mojang runtimes have no IMPLEMENTOR), then by IMPLEMENTOR.
fn classify_java_vendor(home: &std::path::Path, implementor: &str) -> (&'static str, String) {
    let path = home.to_string_lossy().to_lowercase().replace('/', "\\");
    let imp = implementor.to_lowercase();
    let file_name = home.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let id = if path.contains("ika-mojang-")
        || file_name.starts_with("java-runtime-")
        || file_name == "jre-legacy"
        || path.contains("\\.minecraft\\runtime\\")
        || path.contains("\\localcache\\local\\runtime\\")
    {
        "mojang"
    } else if path.contains("ika-adoptium-") || imp.contains("adoptium") || path.contains("temurin") || path.contains("adoptium") {
        "adoptium"
    } else if path.contains("ika-zulu-") || imp.contains("azul") || path.contains("zulu") {
        "zulu"
    } else if path.contains("ika-semeru-") || imp.contains("ibm") || imp.contains("openj9") || path.contains("semeru") {
        "semeru"
    } else if imp.contains("microsoft") || path.contains("\\microsoft\\jdk") {
        "microsoft"
    } else if imp.contains("oracle") || path.contains("\\program files\\java\\") || path.contains("\\program files (x86)\\java\\") {
        "oracle"
    } else {
        "other"
    };
    let name = match id {
        "mojang" => "Mojang".to_string(),
        "adoptium" => "Eclipse Temurin".to_string(),
        "zulu" => "Azul Zulu".to_string(),
        "semeru" => "IBM Semeru Open".to_string(),
        "microsoft" => "Microsoft".to_string(),
        "oracle" => "Oracle".to_string(),
        _ if !implementor.is_empty() => implementor.to_string(),
        _ => "Java".to_string(),
    };
    (id, name)
}

fn java_major_from_version(v: &str) -> u32 {
    let mut parts = v.split(|c: char| c == '.' || c == '_' || c == '+' || c == '-');
    let first = parts.next().unwrap_or("").parse::<u32>().unwrap_or(0);
    if first == 1 {
        parts.next().and_then(|p| p.parse::<u32>().ok()).unwrap_or(0)
    } else {
        first
    }
}

/// Reads version/vendor of a Java home from its `release` file, falling back to `java -version`.
fn probe_java_home(home: &std::path::Path) -> Option<JavaInstallation> {
    let javaw = home.join("bin").join("javaw.exe");
    if !javaw.exists() {
        return None;
    }
    let mut version = String::new();
    let mut full_version = String::new();
    let mut vendor = String::new();
    if let Ok(content) = std::fs::read_to_string(home.join("release")) {
        for line in content.lines() {
            if let Some(v) = line.strip_prefix("SEMANTIC_VERSION=") {
                full_version = v.trim().trim_matches('"').to_string();
            } else if full_version.is_empty() && line.starts_with("JAVA_RUNTIME_VERSION=") {
                if let Some(v) = line.strip_prefix("JAVA_RUNTIME_VERSION=") {
                    full_version = v.trim().trim_matches('"').to_string();
                }
            } else if let Some(v) = line.strip_prefix("JAVA_VERSION=") {
                version = v.trim().trim_matches('"').to_string();
            } else if let Some(v) = line.strip_prefix("IMPLEMENTOR=") {
                vendor = v.trim().trim_matches('"').to_string();
            }
        }
    }

    // Normalizar build number de full_version (ej: "21.0.11+10-LTS" -> "21.0.11.10")
    if !full_version.is_empty() {
        let base = full_version.split('-').next().unwrap_or(&full_version);
        let normalized = base.replace('+', ".");
        if normalized.starts_with(&version) && normalized.len() >= version.len() {
            version = normalized;
        }
    }

    // Si aún no tiene 4 partes y la carpeta contiene un tag tipo +7 o .10
    if version.split('.').count() < 4 {
        let folder_name = home.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if let Some(caps) = regex::Regex::new(r#"(\d+\.\d+\.\d+[\.\+]\d+)"#).ok().and_then(|re| re.captures(&folder_name)) {
            let matched = caps[1].replace('+', ".");
            if matched.starts_with(&version) {
                version = matched;
            }
        }
    }
    if version.is_empty() {
        let java = home.join("bin").join("java.exe");
        if let Ok(out) = create_silent_command(&java.to_string_lossy()).arg("-version").output() {
            let text = String::from_utf8_lossy(&out.stderr);
            if let Some(caps) = regex::Regex::new(r#"version "([^"]+)""#).ok().and_then(|re| re.captures(&text)) {
                version = caps[1].to_string();
            }
        }
    }
    if version.is_empty() {
        return None;
    }
    let (vendor_id, vendor) = classify_java_vendor(home, &vendor);
    Some(JavaInstallation {
        path: javaw.to_string_lossy().to_string(),
        major: java_major_from_version(&version),
        version,
        vendor,
        vendor_id: vendor_id.to_string(),
    })
}

const JAVA_DL_USER_AGENT: &str = "IkaLauncher/1.0 (Windows)";

fn curl_get_text(url: &str) -> Result<String, String> {
    let out = create_silent_command("curl.exe")
        .args(["-s", "-L", "-f", "--max-time", "30", "-A", JAVA_DL_USER_AGENT, url])
        .output()
        .map_err(|e| format!("Error al ejecutar curl: {}", e))?;
    if !out.status.success() {
        return Err(format!("No se pudo consultar {}", url));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn curl_download_file(url: &str, dest: &std::path::Path) -> Result<(), String> {
    let status = create_silent_command("curl.exe")
        .args(["-s", "-L", "-f", "--retry", "2", "-A", JAVA_DL_USER_AGENT, "-o"])
        .arg(dest)
        .arg(url)
        .status()
        .map_err(|e| format!("Error al ejecutar curl: {}", e))?;
    if !status.success() {
        return Err(format!("Falló la descarga de {}", url));
    }
    Ok(())
}

/// Java runtimes downloaded by this launcher (Pinecone/Prism ones are still detected).
fn java_install_root() -> std::path::PathBuf {
    mc::ika_root().join("java")
}

/// Resolves the zip URL of a JRE archive for Adoptium / Zulu / Semeru.
fn java_archive_url(vendor: &str, major: u32) -> Result<String, String> {
    match vendor {
        "adoptium" => Ok(format!(
            "https://api.adoptium.net/v3/binary/latest/{}/ga/windows/x64/jre/hotspot/normal/eclipse",
            major
        )),
        "zulu" => {
            let url = format!(
                "https://api.azul.com/metadata/v1/zulu/packages/?java_version={}&os=windows&arch=x64&archive_type=zip&java_package_type=jre&javafx_bundled=false&latest=true&release_status=ga&availability_types=CA&page_size=1",
                major
            );
            let json: serde_json::Value = serde_json::from_str(&curl_get_text(&url)?).map_err(|e| e.to_string())?;
            json.get(0)
                .and_then(|p| p.get("download_url"))
                .and_then(|u| u.as_str())
                .map(str::to_string)
                .ok_or_else(|| format!("Azul Zulu no ofrece Java {} para Windows", major))
        }
        "semeru" => {
            let url = format!("https://api.github.com/repos/ibmruntimes/semeru{}-binaries/releases/latest", major);
            let json: serde_json::Value = serde_json::from_str(&curl_get_text(&url)?).map_err(|e| e.to_string())?;
            json.get("assets")
                .and_then(|a| a.as_array())
                .and_then(|assets| {
                    assets.iter().find_map(|a| {
                        let name = a.get("name")?.as_str()?;
                        if name.contains("jre_x64_windows") && name.ends_with(".zip") {
                            a.get("browser_download_url")?.as_str().map(str::to_string)
                        } else {
                            None
                        }
                    })
                })
                .ok_or_else(|| format!("IBM Semeru no ofrece Java {} para Windows", major))
        }
        _ => Err(format!("Proveedor de Java desconocido: {}", vendor)),
    }
}

/// Downloads Mojang's official runtime (file-by-file manifest) into `dest`.
fn download_mojang_manifest_files(manifest_url: &str, dest: &std::path::Path) -> Result<(), String> {
    let manifest: serde_json::Value = serde_json::from_str(&curl_get_text(manifest_url)?).map_err(|e| e.to_string())?;
    let files = manifest.get("files").and_then(|f| f.as_object()).ok_or("Manifiesto de archivos inválido")?;

    // One curl process downloading everything in parallel, driven by a config file
    let mut config = String::new();
    for (rel, entry) in files {
        let target = dest.join(rel);
        match entry.get("type").and_then(|t| t.as_str()) {
            Some("directory") => {
                std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            }
            Some("file") => {
                if let Some(url) = entry.pointer("/downloads/raw/url").and_then(|u| u.as_str()) {
                    let out = target.to_string_lossy().replace('\\', "/");
                    config.push_str(&format!("url = \"{}\"\noutput = \"{}\"\n", url, out));
                }
            }
            _ => {}
        }
    }
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let config_path = dest.join(".ika-download.cfg");
    std::fs::write(&config_path, config).map_err(|e| e.to_string())?;
    let status = create_silent_command("curl.exe")
        .args(["-s", "-f", "-L", "--retry", "2", "--create-dirs", "--parallel", "--parallel-max", "16", "-A", JAVA_DL_USER_AGENT, "-K"])
        .arg(&config_path)
        .status()
        .map_err(|e| format!("Error al ejecutar curl: {}", e))?;
    let _ = std::fs::remove_file(&config_path);
    if !status.success() {
        return Err("Falló la descarga del runtime de Mojang".to_string());
    }
    Ok(())
}

fn download_mojang_runtime(major: u32, dest: &std::path::Path, direct_manifest: Option<&str>) -> Result<(), String> {
    if let Some(manifest_url) = direct_manifest {
        if !manifest_url.trim().is_empty() {
            return download_mojang_manifest_files(manifest_url, dest);
        }
    }

    let all: serde_json::Value = serde_json::from_str(&curl_get_text(
        "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json",
    )?)
    .map_err(|e| e.to_string())?;
    let components = all
        .get("windows-x64")
        .and_then(|c| c.as_object())
        .ok_or("Manifiesto de Java de Mojang inválido")?;

    // Version names look like "21.0.7" or "8u51-cacert462b08": compare leading digits only
    let leading_major = |name: &str| -> u32 {
        let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
        let n = digits.parse::<u32>().unwrap_or(0);
        if n == 1 { java_major_from_version(name) } else { n }
    };
    let mut keys: Vec<&String> = components
        .iter()
        .filter(|(k, v)| {
            !k.contains("snapshot")
                && v.get(0)
                    .and_then(|e| e.pointer("/version/name"))
                    .and_then(|n| n.as_str())
                    .map(|n| leading_major(n) == major)
                    .unwrap_or(false)
        })
        .map(|(k, _)| k)
        .collect();
    keys.sort();
    let key = keys.last().ok_or_else(|| format!("Mojang no ofrece Java {}", major))?;
    let manifest_url = components[key.as_str()]
        .pointer("/0/manifest/url")
        .and_then(|u| u.as_str())
        .ok_or("Manifiesto de Java de Mojang inválido")?;

    download_mojang_manifest_files(manifest_url, dest)
}

/// Folder for a runtime: like Prism/Pinecone (`javaPath/<runtime name>`), the meta
/// runtime's own name, e.g. "eclipse_temurin_jre21.0.12+8" or "java-runtime-delta". Each
/// build gets its own folder, and Pinecone and this launcher recognise each other's
/// downloads. Must match `javaRuntimeFolder` in main.js.
fn java_runtime_folder(vendor: &str, major: u32, runtime_name: Option<&str>) -> String {
    let clean: String = runtime_name
        .unwrap_or("")
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    let clean = clean.trim_end_matches(['.', ' ']).trim();
    if clean.is_empty() {
        format!("ika-{}-{}", vendor, major)
    } else {
        clean.to_string()
    }
}

fn download_java_runtime_inner(
    vendor: &str,
    major: u32,
    direct_url: Option<String>,
    runtime_name: Option<String>,
) -> Result<JavaInstallation, String> {
    let root = java_install_root();
    std::fs::create_dir_all(&root).map_err(|e| format!("No se pudo crear la carpeta de Java: {}", e))?;
    let folder = java_runtime_folder(vendor, major, runtime_name.as_deref());
    let final_dir = root.join(&folder);
    let staging = root.join(format!(".tmp-{}", folder));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    let result = (|| -> Result<std::path::PathBuf, String> {
        if vendor == "mojang" {
            download_mojang_runtime(major, &staging, direct_url.as_deref())?;
            return Ok(staging.clone());
        }
        let url = match direct_url {
            Some(ref u) if !u.trim().is_empty() => u.clone(),
            _ => java_archive_url(vendor, major)?,
        };
        let zip_path = staging.join("java.zip");
        curl_download_file(&url, &zip_path)?;
        let file = std::fs::File::open(&zip_path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Archivo de Java inválido: {}", e))?;
        archive.extract(&staging).map_err(|e| format!("Error al extraer Java: {}", e))?;
        let _ = std::fs::remove_file(&zip_path);
        // Archives contain a top-level folder (e.g. jdk-21.0.5+11-jre)
        let mut homes = Vec::new();
        collect_java_homes(&staging, 2, &mut homes);
        homes.into_iter().next().ok_or_else(|| "El archivo descargado no contiene javaw.exe".to_string())
    })();

    let home = match result {
        Ok(h) => h,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }
    };

    let _ = std::fs::remove_dir_all(&final_dir);
    std::fs::rename(&home, &final_dir).map_err(|e| format!("No se pudo instalar Java: {}", e))?;
    let _ = std::fs::remove_dir_all(&staging);
    probe_java_home(&final_dir).ok_or_else(|| "Java se descargó pero no se pudo verificar".to_string())
}

#[tauri::command]
async fn download_java_runtime(
    vendor: String,
    major: u32,
    direct_url: Option<String>,
    runtime_name: Option<String>,
) -> Result<JavaInstallation, String> {
    tauri::async_runtime::spawn_blocking(move || download_java_runtime_inner(&vendor, major, direct_url, runtime_name))
        .await
        .map_err(|e| e.to_string())?
}

fn collect_java_homes(dir: &std::path::Path, depth: u32, out: &mut Vec<std::path::PathBuf>) {
    if dir.join("bin").join("javaw.exe").exists() {
        out.push(dir.to_path_buf());
        return;
    }
    if depth == 0 {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                collect_java_homes(&entry.path(), depth - 1, out);
            }
        }
    }
}

#[tauri::command(async)]
fn list_java_installations() -> Vec<JavaInstallation> {
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let program_files = env("ProgramFiles");
    let program_files_x86 = env("ProgramFiles(x86)");
    let appdata = env("APPDATA");
    let local_appdata = env("LOCALAPPDATA");
    let user_profile = env("USERPROFILE");

    let mut roots: Vec<(std::path::PathBuf, u32)> = Vec::new();
    for vendor_dir in ["Java", "Eclipse Adoptium", "Eclipse Foundation", "Microsoft", "Zulu", "BellSoft", "Amazon Corretto", "Semeru", "OpenJDK"] {
        roots.push((std::path::Path::new(&program_files).join(vendor_dir), 2));
        roots.push((std::path::Path::new(&program_files_x86).join(vendor_dir), 2));
    }
    roots.push((java_install_root(), 3));
    roots.push((std::path::Path::new(&appdata).join("ElyPrismLauncher").join("java"), 3));
    roots.push((std::path::Path::new(&appdata).join("PrismLauncher").join("java"), 3));
    roots.push((std::path::Path::new(&appdata).join("FreesmLauncher").join("java"), 3));
    roots.push((std::path::Path::new(&appdata).join(".battly").join("runtime"), 4));
    roots.push((std::path::Path::new(&appdata).join(".minecraft").join("runtime"), 4));
    roots.push((
        std::path::Path::new(&local_appdata)
            .join("Packages")
            .join("Microsoft.4297127D64EC6_8wekyb3d8bbwe")
            .join("LocalCache")
            .join("Local")
            .join("runtime"),
        4,
    ));
    roots.push((std::path::Path::new(&user_profile).join(".jdks"), 2));
    let java_home = env("JAVA_HOME");
    if !java_home.is_empty() {
        roots.push((std::path::PathBuf::from(java_home), 0));
    }

    let mut homes = Vec::new();
    for (root, depth) in &roots {
        if root.as_os_str().is_empty() || !root.exists() {
            continue;
        }
        collect_java_homes(root, *depth, &mut homes);
    }

    let mut seen = std::collections::HashSet::new();
    let mut result: Vec<JavaInstallation> = homes
        .into_iter()
        .filter(|h| seen.insert(h.to_string_lossy().to_lowercase().trim_end_matches(['\\', '/']).to_string()))
        .filter_map(|h| probe_java_home(&h))
        .collect();
    result.sort_by(|a, b| b.major.cmp(&a.major).then_with(|| a.path.cmp(&b.path)));
    result
}

#[tauri::command]
fn pick_java_executable() -> Option<JavaInstallation> {
    let file = rfd::FileDialog::new()
        .set_title("Seleccionar ejecutable de Java (javaw.exe)")
        .add_filter("Java", &["exe"])
        .pick_file()?;
    // javaw.exe lives in <home>/bin
    let home = file.parent()?.parent()?;
    probe_java_home(home).map(|mut j| {
        j.path = file.to_string_lossy().to_string();
        j
    })
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct MemoryStatusEx {
    length: u32,
    memory_load: u32,
    total_phys: u64,
    avail_phys: u64,
    total_page_file: u64,
    avail_page_file: u64,
    total_virtual: u64,
    avail_virtual: u64,
    avail_extended_virtual: u64,
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
}

/// Total physical RAM in MB (0 if unknown).
#[tauri::command(async)]
fn get_system_memory_mb() -> u64 {
    #[cfg(target_os = "windows")]
    {
        let mut status = MemoryStatusEx {
            length: std::mem::size_of::<MemoryStatusEx>() as u32,
            memory_load: 0,
            total_phys: 0,
            avail_phys: 0,
            total_page_file: 0,
            avail_page_file: 0,
            total_virtual: 0,
            avail_virtual: 0,
            avail_extended_virtual: 0,
        };
        // SAFETY: `status` is a properly sized, initialized MEMORYSTATUSEX with `length` set.
        if unsafe { GlobalMemoryStatusEx(&mut status) } != 0 {
            return status.total_phys / (1024 * 1024);
        }
    }
    0
}

/// Escapes a value for Prism's instance.cfg INI format.
fn escape_prism_ini_value(v: &str) -> String {
    v.replace('\\', "\\\\").replace('\n', " ").replace('\r', "")
}

/// Loader components of an instance (uid in mmc-pack.json -> name shown in the UI).
const LOADER_COMPONENTS: [(&str, &str); 4] = [
    ("net.fabricmc.fabric-loader", "Fabric"),
    ("org.quiltmc.quilt-loader", "Quilt"),
    ("net.minecraftforge", "Forge"),
    ("net.neoforged", "NeoForge"),
];

fn instance_root_dir(instance_path: &str) -> Result<std::path::PathBuf, String> {
    std::path::Path::new(instance_path)
        .ancestors()
        .take(4)
        .find(|p| {
            p.join("mmc-pack.json").exists()
                || p.join("instance.cfg").exists()
                || p.join("instance.json").exists()
                || p.join(".oneclient-materialized.json").exists()
                || p.join(".oneclient-mods.json").exists()
                || p.join("base-modpack.mrpack").exists()
                || p.join("mods").is_dir()
        })
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "No se encontró la instancia".to_string())
}

fn read_instance_cfg(dir: &std::path::Path) -> Vec<(String, String)> {
    std::fs::read_to_string(dir.join("instance.cfg"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .collect()
}

/// Sets and removes keys in instance.cfg, keeping every other line as it was.
fn edit_instance_cfg(dir: &std::path::Path, set: &[(&str, String)], remove: &[&str]) -> Result<(), String> {
    let path = dir.join("instance.cfg");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| {
            let key = l.split_once('=').map(|(k, _)| k.trim()).unwrap_or("");
            !remove.contains(&key)
        })
        .map(str::to_string)
        .collect();
    for (key, value) in set {
        let entry = format!("{}={}", key, value);
        match lines.iter().position(|l| l.split_once('=').map_or(false, |(k, _)| k.trim() == *key)) {
            Some(i) => lines[i] = entry,
            None => lines.push(entry),
        }
    }
    std::fs::write(&path, lines.join("\n") + "\n").map_err(|e| e.to_string())
}

/// What the edit window shows: the instance's real settings.
#[derive(serde::Serialize)]
pub struct InstanceDetails {
    path: String,
    name: String,
    mc_version: String,
    loader: String,
    loader_version: Option<String>,
    java_path: Option<String>,
    java_vendor: Option<String>,
    java_major: Option<u32>,
    java_runtime: Option<String>,
    jvm_args: String,
    max_mem_mb: Option<u32>,
    min_mem_mb: Option<u32>,
    mods_count: usize,
}

#[tauri::command(async)]
fn get_instance_details(instance_path: String) -> Result<InstanceDetails, String> {
    let dir = instance_root_dir(&instance_path)?;
    let pack: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("mmc-pack.json")).map_err(|e| e.to_string())?)
        .map_err(|e| format!("mmc-pack.json inválido: {}", e))?;
    let comps = pack.get("components").and_then(|c| c.as_array()).cloned().unwrap_or_default();
    let comp_version = |c: &serde_json::Value| {
        c.get("version").and_then(|v| v.as_str()).or_else(|| c.get("cachedVersion").and_then(|v| v.as_str())).map(str::to_string)
    };
    let mc_version = comps
        .iter()
        .find(|c| c.get("uid").and_then(|u| u.as_str()) == Some("net.minecraft"))
        .and_then(comp_version)
        .unwrap_or_default();
    let (loader, loader_version) = comps
        .iter()
        .find_map(|c| {
            let uid = c.get("uid").and_then(|u| u.as_str())?;
            LOADER_COMPONENTS.iter().find(|(u, _)| *u == uid).map(|(_, name)| (name.to_string(), comp_version(c)))
        })
        .unwrap_or_else(|| ("Vanilla".to_string(), None));

    let cfg = read_instance_cfg(&dir);
    let get = |k: &str| cfg.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
    let override_java = get("OverrideJavaLocation").map_or(false, |v| v == "true");
    let java_path = get("JavaPath").filter(|p| !p.is_empty() && (override_java || get("OverrideJavaLocation").is_none()));
    let override_mem = get("OverrideMemory").map_or(false, |v| v == "true");
    let jvm_args = if get("OverrideJavaArgs").map_or(true, |v| v == "true") {
        get("JvmArgs").map(|a| a.replace("\\\\", "\\")).unwrap_or_default()
    } else {
        String::new()
    };
    let game_dir = if dir.join(".minecraft").exists() { dir.join(".minecraft") } else { dir.join("minecraft") };
    let mods_count = std::fs::read_dir(game_dir.join("mods"))
        .map(|rd| rd.flatten().filter(|e| e.file_name().to_string_lossy().to_lowercase().ends_with(".jar")).count())
        .unwrap_or(0);

    Ok(InstanceDetails {
        path: dir.to_string_lossy().to_string(),
        name: get("name").filter(|n| !n.is_empty()).unwrap_or_else(|| dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
        mc_version,
        loader,
        loader_version,
        java_path,
        java_vendor: get("IkaJavaVendor").filter(|v| !v.is_empty()),
        java_major: get("IkaJavaMajor").and_then(|m| m.parse().ok()),
        java_runtime: get("IkaJavaRuntime").filter(|v| !v.is_empty()),
        jvm_args,
        max_mem_mb: if override_mem { get("MaxMemAlloc").and_then(|m| m.parse().ok()) } else { None },
        min_mem_mb: if override_mem { get("MinMemAlloc").and_then(|m| m.parse().ok()) } else { None },
        mods_count,
    })
}

/// Only the fields the player changed in the edit window.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceChanges {
    name: Option<String>,
    mc_version: Option<String>,
    /// Vanilla | Fabric | Quilt | Forge | NeoForge (sent together with loader_version)
    loader: Option<String>,
    loader_version: Option<String>,
    java_path: Option<String>,
    java_runtime: Option<JavaRuntimeChoice>,
    jvm_args: Option<String>,
    max_mem_mb: Option<u32>,
}

#[tauri::command(async)]
fn update_instance(instance_path: String, changes: InstanceChanges) -> Result<String, String> {
    let dir = instance_root_dir(&instance_path)?;
    let mut set: Vec<(&str, String)> = Vec::new();
    let mut remove: Vec<&str> = Vec::new();

    if let Some(name) = changes.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        set.push(("name", escape_prism_ini_value(name)));
    }

    // Version / loader: rewrite the managed components (Minecraft, loader, and the
    // dependency-only ones the resolver adds back: LWJGL, Intermediary). Others
    // (custom patches, jar mods) are kept.
    if changes.mc_version.is_some() || changes.loader.is_some() {
        let pack_path = dir.join("mmc-pack.json");
        let mut pack: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&pack_path).map_err(|e| e.to_string())?)
            .map_err(|e| format!("mmc-pack.json inválido: {}", e))?;
        let comps = pack.get("components").and_then(|c| c.as_array()).cloned().unwrap_or_default();
        let current = get_instance_details(instance_path.clone())?;
        let mc = changes.mc_version.clone().unwrap_or(current.mc_version.clone());
        let (loader, loader_version) = match &changes.loader {
            Some(l) => (l.clone(), changes.loader_version.clone()),
            None => (current.loader.clone(), current.loader_version.clone()),
        };
        let managed = |uid: &str| {
            uid == "net.minecraft"
                || uid == "org.lwjgl"
                || uid == "org.lwjgl3"
                || uid == "net.fabricmc.intermediary"
                || LOADER_COMPONENTS.iter().any(|(u, _)| *u == uid)
        };
        let mut new_comps = vec![serde_json::json!({
            "cachedName": "Minecraft", "cachedVersion": mc, "important": true, "uid": "net.minecraft", "version": mc
        })];
        if loader != "Vanilla" {
            let (uid, cached_name) = match loader.as_str() {
                "Fabric" => ("net.fabricmc.fabric-loader", "Fabric Loader"),
                "Quilt" => ("org.quiltmc.quilt-loader", "Quilt Loader"),
                "Forge" => ("net.minecraftforge", "Forge"),
                "NeoForge" => ("net.neoforged", "NeoForge"),
                other => return Err(format!("Loader desconocido: {}", other)),
            };
            let ver = loader_version.filter(|v| !v.is_empty()).ok_or("Falta la versión del loader")?;
            new_comps.push(serde_json::json!({ "cachedName": cached_name, "cachedVersion": ver, "uid": uid, "version": ver }));
        }
        new_comps.extend(comps.into_iter().filter(|c| !managed(c.get("uid").and_then(|u| u.as_str()).unwrap_or(""))));
        pack["components"] = serde_json::Value::Array(new_comps);
        std::fs::write(&pack_path, serde_json::to_string_pretty(&pack).unwrap_or_default()).map_err(|e| e.to_string())?;
        set.push(("IntendedVersion", mc));
        // Natives of the old version would be stale.
        let _ = std::fs::remove_dir_all(dir.join("natives"));
    }

    // Java: an installed one (path) or a runtime downloaded before the next launch.
    if let Some(jp) = changes.java_path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        set.push(("OverrideJavaLocation", "true".to_string()));
        set.push(("JavaPath", jp.replace('\\', "/")));
        remove.extend(["IkaJavaVendor", "IkaJavaMajor", "IkaJavaRuntime", "IkaJavaUrl"]);
    } else if let Some(rt) = changes.java_runtime.as_ref().filter(|r| !r.vendor.trim().is_empty()) {
        set.push(("IkaJavaVendor", rt.vendor.trim().to_string()));
        set.push(("IkaJavaMajor", rt.major.to_string()));
        set.push(("IkaJavaRuntime", rt.runtime_name.clone().unwrap_or_default().trim().to_string()));
        set.push(("IkaJavaUrl", rt.download_url.clone().unwrap_or_default().trim().to_string()));
        remove.extend(["JavaPath", "OverrideJavaLocation"]);
    }

    if let Some(args) = changes.jvm_args.as_ref() {
        set.push(("OverrideJavaArgs", "true".to_string()));
        set.push(("JvmArgs", escape_prism_ini_value(args.trim())));
    }
    if let Some(max) = changes.max_mem_mb {
        set.push(("OverrideMemory", "true".to_string()));
        set.push(("MaxMemAlloc", max.to_string()));
        set.push(("MinMemAlloc", 512.min(max).to_string()));
    }

    edit_instance_cfg(&dir, &set, &remove)?;
    Ok(dir.to_string_lossy().to_string())
}

/// Java picked in the creation screen that isn't on the PC yet (vendor + exact build).
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaRuntimeChoice {
    vendor: String,
    major: u32,
    runtime_name: Option<String>,
    download_url: Option<String>,
}

#[tauri::command]
async fn create_new_instance(
    app: tauri::AppHandle,
    name: String,
    mc_version: String,
    loader: String,
    loader_version: Option<String>,
    java_version: String,
    java_path: Option<String>,
    java_runtime: Option<JavaRuntimeChoice>,
    jvm_args: Option<String>,
    min_mem_mb: Option<u32>,
    max_mem_mb: Option<u32>,
    modpack: Option<mc::ModpackInstallSpec>,
) -> Result<LauncherInstance, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let clean_name = name.trim();
        if clean_name.is_empty() {
            return Err("El nombre de la instancia no puede estar vacío.".to_string());
        }

        // New instances always go to the launcher's own folder.
        let base = mc::instances_root();
        std::fs::create_dir_all(&base).map_err(|e| format!("No se pudo crear la carpeta de instancias: {}", e))?;

        let inst_dir = base.join(clean_name);
        if inst_dir.exists() {
            return Err("Ya existe una instancia con ese nombre.".to_string());
        }

        let mc_dir = inst_dir.join(".minecraft");
        let mods_dir = mc_dir.join("mods");
        std::fs::create_dir_all(&mods_dir).map_err(|e| format!("Error al crear carpeta de mods: {}", e))?;

        let mut final_mc_version = mc_version.trim().to_string();
        let mut final_loader = loader.trim().to_string();
        let mut final_loader_ver = loader_version
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let mut managed_pack_meta: Option<(String, String, String)> = None;

        if let Some(spec) = &modpack {
            match mc::install_modpack(
                Some(app),
                clean_name,
                &mc_dir,
                spec,
                &final_mc_version,
                &final_loader,
            ) {
                Ok(outcome) => {
                    final_mc_version = outcome.mc_version;
                    final_loader = outcome.loader;
                    if outcome.loader_version.is_some() {
                        final_loader_ver = outcome.loader_version;
                    }
                    managed_pack_meta = Some((outcome.pack_type, outcome.pack_id, outcome.pack_name));
                }
                Err(e) => {
                    let _ = std::fs::remove_dir_all(&inst_dir);
                    return Err(e);
                }
            }
        }

        if final_loader_ver.is_none() {
            final_loader_ver = mc::resolve_loader_version(&final_mc_version, &final_loader, None);
        }

        let cfg_path = inst_dir.join("instance.cfg");
        let mut cfg_content = format!(
            "InstanceType=OneSix\nname={}\nIntendedVersion={}\nIntendedLoader={}\niconKey=default\nnotes=\n",
            clean_name, final_mc_version, final_loader
        );
        if let Some((pack_type, pack_id, pack_name)) = &managed_pack_meta {
            cfg_content.push_str(&format!(
                "ManagedPack=true\nManagedPackType={}\nManagedPackID={}\nManagedPackName={}\n",
                escape_prism_ini_value(pack_type),
                escape_prism_ini_value(pack_id),
                escape_prism_ini_value(pack_name),
            ));
        }
        let java_path = java_path.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        if let Some(jp) = &java_path {
            // Prism stores paths with forward slashes
            cfg_content.push_str(&format!("OverrideJavaLocation=true\nJavaPath={}\n", jp.replace('\\', "/")));
        } else if let Some(rt) = java_runtime.as_ref().filter(|r| !r.vendor.trim().is_empty()) {
            // A runtime that isn't installed yet: mc.rs downloads it before the first launch.
            cfg_content.push_str(&format!(
                "IkaJavaVendor={}\nIkaJavaMajor={}\nIkaJavaRuntime={}\nIkaJavaUrl={}\n",
                rt.vendor.trim(),
                rt.major,
                escape_prism_ini_value(rt.runtime_name.as_deref().unwrap_or("").trim()),
                rt.download_url.as_deref().unwrap_or("").trim(),
            ));
        }
        if let Some(args) = jvm_args.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
            cfg_content.push_str(&format!("OverrideJavaArgs=true\nJvmArgs={}\n", escape_prism_ini_value(args)));
        }
        if let Some(max) = max_mem_mb {
            let min = min_mem_mb.unwrap_or(512).min(max);
            cfg_content.push_str(&format!("OverrideMemory=true\nMinMemAlloc={}\nMaxMemAlloc={}\n", min, max));
        }
        let _ = std::fs::write(&cfg_path, cfg_content);

        let pack_path = inst_dir.join("mmc-pack.json");
        let mut components = vec![serde_json::json!({
            "cachedName": "Minecraft",
            "cachedVersion": &final_mc_version,
            "important": true,
            "uid": "net.minecraft",
            "version": &final_mc_version
        })];

        if let Some((uid, cached_name, canonical_loader)) = mc::loader_uid_and_name(&final_loader) {
            final_loader = canonical_loader.to_string();
            let ver = final_loader_ver
                .clone()
                .or_else(|| mc::resolve_loader_version(&final_mc_version, &final_loader, None))
                .unwrap_or_default();
            if !ver.is_empty() {
                components.push(serde_json::json!({
                    "cachedName": cached_name,
                    "cachedVersion": &ver,
                    "uid": uid,
                    "version": &ver
                }));
            }
        }

        let pack_json = serde_json::json!({
            "components": components,
            "formatVersion": 1
        });
        let _ = std::fs::write(&pack_path, serde_json::to_string_pretty(&pack_json).unwrap_or_default());

        let mod_count = std::fs::read_dir(&mods_dir)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .to_lowercase()
                            .ends_with(".jar")
                    })
                    .count()
            })
            .unwrap_or(0);
        let shader_count = std::fs::read_dir(mc_dir.join("shaderpacks"))
            .map(|rd| {
                rd.flatten()
                    .filter(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .to_lowercase()
                            .ends_with(".zip")
                    })
                    .count()
            })
            .unwrap_or(0);
        let resourcepack_count = std::fs::read_dir(mc_dir.join("resourcepacks"))
            .map(|rd| {
                rd.flatten()
                    .filter(|e| {
                        let p = e.path();
                        p.is_dir()
                            || p.file_name()
                                .map_or(false, |n| n.to_string_lossy().to_lowercase().ends_with(".zip"))
                    })
                    .count()
            })
            .unwrap_or(0);

        Ok(LauncherInstance {
            name: clean_name.to_string(),
            folder_name: clean_name.to_string(),
            path: inst_dir.to_string_lossy().to_string(),
            mods_path: mods_dir.to_string_lossy().to_string(),
            mod_count,
            mc_version: final_mc_version,
            loader: final_loader,
            icon_path: None,
            java_version: Some(java_version),
            java_vendor: None,
            java_path,
            total_time_played: Some(0),
            shader_count,
            resourcepack_count,
            max_mem_mb,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct AccountSkinInfo {
    pub username: String,
    pub skin_data_url: Option<String>,
    pub skin_model: String,
}

#[tauri::command(async)]
fn get_account_skin(username: String) -> Option<AccountSkinInfo> {
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    let paths = [
        format!("{}\\ElyPrismLauncher\\accounts.json", appdata),
        format!("{}\\PrismLauncher\\accounts.json", appdata),
        format!("{}\\FreesmLauncher\\accounts.json", appdata),
    ];
    let user_lower = username.trim().to_lowercase();
    for p in &paths {
        if let Ok(content) = std::fs::read_to_string(p) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(accounts) = v.get("accounts").and_then(|a| a.as_array()) {
                    for acc in accounts {
                        let name = acc.pointer("/profile/name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("");
                        if name.trim().to_lowercase() == user_lower {
                            let skin_data = acc.pointer("/profile/skin/data")
                                .and_then(|d| d.as_str())
                                .filter(|s| !s.trim().is_empty())
                                .map(|d| format!("data:image/png;base64,{}", d));
                            let variant = acc.pointer("/profile/skin/variant")
                                .and_then(|va| va.as_str())
                                .unwrap_or("CLASSIC");
                            let skin_model = if variant.eq_ignore_ascii_case("SLIM") {
                                "slim".to_string()
                            } else {
                                "default".to_string()
                            };
                            return Some(AccountSkinInfo {
                                username: name.to_string(),
                                skin_data_url: skin_data,
                                skin_model,
                            });
                        }
                    }
                }
            }
        }
    }
    None
}

#[tauri::command(async)]
fn get_discovered_accounts() -> Vec<AccountSkinInfo> {
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    let paths = [
        format!("{}\\ElyPrismLauncher\\accounts.json", appdata),
        format!("{}\\PrismLauncher\\accounts.json", appdata),
        format!("{}\\FreesmLauncher\\accounts.json", appdata),
    ];
    let mut results: Vec<AccountSkinInfo> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for p in &paths {
        if let Ok(content) = std::fs::read_to_string(p) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(accounts) = v.get("accounts").and_then(|a| a.as_array()) {
                    for acc in accounts {
                        let name = acc.pointer("/profile/name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("");
                        if !name.trim().is_empty() {
                            let key = name.trim().to_lowercase();
                            if !seen.contains(&key) {
                                seen.insert(key);
                                let skin_data = acc.pointer("/profile/skin/data")
                                    .and_then(|d| d.as_str())
                                    .filter(|s| !s.trim().is_empty())
                                    .map(|d| format!("data:image/png;base64,{}", d));
                                let variant = acc.pointer("/profile/skin/variant")
                                    .and_then(|va| va.as_str())
                                    .unwrap_or("CLASSIC");
                                let skin_model = if variant.eq_ignore_ascii_case("SLIM") {
                                    "slim".to_string()
                                } else {
                                    "default".to_string()
                                };
                                results.push(AccountSkinInfo {
                                    username: name.to_string(),
                                    skin_data_url: skin_data,
                                    skin_model,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    results
}

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// RivaTuner Statistics Server (MSI Afterburner's overlay) injects into every process and,
/// in the launcher, burned ~4.5 s of CPU on the UI thread before WebView2 could start
/// (startup 5.6 s → 0.6 s once excluded). Blocking the injection doesn't help (RTSS then
/// waits/retries even longer); what works is RTSS's own per-application profile with hooking
/// disabled: `<exe name>.cfg` containing `[Hooking] EnableHooking=0` in its Profiles folder.
/// That folder is under Program Files, so the profile is written by an elevated instance of
/// the launcher started with RTSS_EXCLUDE_ARG, after the user agreed in the UI.
#[cfg(target_os = "windows")]
mod rtss {
    use std::path::PathBuf;

    pub const RTSS_EXCLUDE_ARG: &str = "--rtss-exclude-self";
    const PROFILE: &str = "[Hooking]\r\nEnableHooking=0\r\n";

    #[link(name = "advapi32")]
    extern "system" {
        fn RegGetValueW(
            hkey: isize,
            sub_key: *const u16,
            value: *const u16,
            flags: u32,
            kind: *mut u32,
            data: *mut std::ffi::c_void,
            len: *mut u32,
        ) -> i32;
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct SHELLEXECUTEINFOW {
        cbSize: u32,
        fMask: u32,
        hwnd: isize,
        lpVerb: *const u16,
        lpFile: *const u16,
        lpParameters: *const u16,
        lpDirectory: *const u16,
        nShow: i32,
        hInstApp: isize,
        lpIDList: *mut std::ffi::c_void,
        lpClass: *const u16,
        hkeyClass: isize,
        dwHotKey: u32,
        hIconOrMonitor: isize,
        hProcess: isize,
    }

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteExW(info: *mut SHELLEXECUTEINFOW) -> i32;
    }

    type HANDLE = *mut std::ffi::c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn WaitForSingleObject(handle: HANDLE, ms: u32) -> u32;
        fn GetExitCodeProcess(handle: HANDLE, code: *mut u32) -> i32;
        fn CloseHandle(handle: HANDLE) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn install_dir() -> Option<PathBuf> {
        const HKEY_LOCAL_MACHINE: isize = 0x80000002u32 as i32 as isize;
        const RRF_RT_REG_SZ: u32 = 0x2;
        let key = wide(r"SOFTWARE\WOW6432Node\Unwinder\RTSS");
        let name = wide("InstallDir");
        let mut buf = vec![0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        let ok = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut _,
                &mut len,
            )
        } == 0;
        let from_registry = ok
            .then(|| {
                let n = (len as usize / 2).saturating_sub(1);
                PathBuf::from(String::from_utf16_lossy(&buf[..n]))
            })
            .filter(|p| p.join("RTSS.exe").exists());
        from_registry.or_else(|| {
            let default = PathBuf::from(std::env::var("ProgramFiles(x86)").ok()?).join("RivaTuner Statistics Server");
            default.join("RTSS.exe").exists().then_some(default)
        })
    }

    pub fn exe_name() -> Option<String> {
        Some(std::env::current_exe().ok()?.file_name()?.to_string_lossy().to_string())
    }

    fn profile_path() -> Option<PathBuf> {
        Some(install_dir()?.join("Profiles").join(format!("{}.cfg", exe_name()?)))
    }

    /// (RTSS installed, launcher already has a profile there)
    pub fn status() -> (bool, bool) {
        match profile_path() {
            Some(p) => (true, p.exists()),
            None => (false, false),
        }
    }

    /// Elevated helper mode: write the profile (never overwrite a user's own) and report.
    pub fn write_profile() -> bool {
        let Some(path) = profile_path() else { return false };
        if path.exists() {
            return true;
        }
        std::fs::write(&path, PROFILE).is_ok()
    }

    /// Relaunches this executable elevated (UAC prompt) in helper mode and waits for it.
    pub fn exclude_via_elevated_helper() -> bool {
        let Ok(exe) = std::env::current_exe() else { return false };
        const SEE_MASK_NOCLOSEPROCESS: u32 = 0x40;
        let verb = wide("runas");
        let file = wide(&exe.to_string_lossy());
        let params = wide(RTSS_EXCLUDE_ARG);
        let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = 0; // SW_HIDE
        if unsafe { ShellExecuteExW(&mut info) } == 0 || info.hProcess == 0 {
            return false; // UAC declined or failed
        }
        let process = info.hProcess as HANDLE;
        let mut code = 1u32;
        unsafe {
            WaitForSingleObject(process, 30_000);
            GetExitCodeProcess(process, &mut code);
            CloseHandle(process);
        }
        code == 0 && status().1
    }
}

#[derive(serde::Serialize)]
struct RtssStatus {
    installed: bool,
    excluded: bool,
    /// The profile is per executable name, so the "already asked" flag must be too.
    exe: String,
}

#[tauri::command(async)]
fn rtss_status() -> RtssStatus {
    #[cfg(target_os = "windows")]
    {
        let (installed, excluded) = rtss::status();
        return RtssStatus { installed, excluded, exe: rtss::exe_name().unwrap_or_default() };
    }
    #[cfg(not(target_os = "windows"))]
    RtssStatus { installed: false, excluded: false, exe: String::new() }
}

/// Shows the UAC prompt; async so the UI keeps running while the user decides.
#[tauri::command(async)]
fn rtss_exclude_launcher() -> bool {
    #[cfg(target_os = "windows")]
    return rtss::exclude_via_elevated_helper();
    #[cfg(not(target_os = "windows"))]
    false
}

/// The interface is designed on a 2560×1440 screen at 100%. On any other screen the whole page
/// is scaled the way a picture would be (the webview's zoom), so everything keeps its place and
/// its proportions instead of being laid out again for a smaller area: 0.75 on a 1920×1080
/// screen, 1.5 on a 3840×2160 one at 100%, 1 on that same one at 150%.
const UI_REFERENCE_SCREEN: (f64, f64) = (2560.0, 1440.0);
/// The zoom the page has now (0: not set yet).
static UI_ZOOM: Mutex<f64> = Mutex::new(0.0);

/// Sets the page's zoom for the screen the window is on. Called when the window is created and
/// whenever it may have changed screen (moved, resized, the screen's scale changed).
fn apply_ui_zoom(window: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = window.current_monitor() else { return };
    let scale = monitor.scale_factor().max(0.1);
    let width = monitor.size().width as f64 / scale;
    let height = monitor.size().height as f64 / scale;
    // not below 0.6: on a very small screen the text would stop being readable
    let zoom = (width / UI_REFERENCE_SCREEN.0).min(height / UI_REFERENCE_SCREEN.1).clamp(0.6, 2.5);
    let zoom = (zoom * 1000.0).round() / 1000.0;
    let mut current = UI_ZOOM.lock().unwrap_or_else(|e| e.into_inner());
    if (*current - zoom).abs() < 0.0005 {
        return;
    }
    if window.set_zoom(zoom).is_ok() {
        *current = zoom;
    }
}

pub fn run() {
    // Elevated helper mode (see `rtss`): write the RTSS profile and exit before any
    // window, plugin or single-instance check runs.
    #[cfg(target_os = "windows")]
    if std::env::args().any(|a| a == rtss::RTSS_EXCLUDE_ARG) {
        std::process::exit(if rtss::write_profile() { 0 } else { 1 });
    }

    #[cfg(target_os = "windows")]
    {
        // Clean up stale lockfile if left over from an unexpected crash/kill
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let lockfile = std::path::PathBuf::from(local_app_data)
                .join("com.modcomparer.desktop")
                .join("EBWebView")
                .join("lockfile");
            if lockfile.exists() {
                let _ = std::fs::remove_file(&lockfile);
            }
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_log::Builder::default().build())
        .setup(|app| {
            #[cfg(target_os = "windows")]
            we_freeze::recover_after_crash();
            if let Some(main_win) = app.get_webview_window("main") {
                disable_browser_shortcuts(&main_win);
                // Reopen where and how the user left the window, while it is still hidden, so
                // the page lays itself out for that size from the start.
                #[cfg(target_os = "windows")]
                if let Ok(h) = main_win.hwnd() {
                    let had_state = window_state::restore(h.0 as isize);
                    if had_state || !window_state::has_saved_state() {
                        let _ = main_win.maximize();
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = main_win.maximize();
                }
                // after the window is back on its screen: the zoom depends on which one it is
                apply_ui_zoom(&main_win);
                if let Some(icon) = app.default_window_icon() {
                    let _ = main_win.set_icon(icon.clone());
                }
                // Fallback guarantee: reveal window if frontend doesn't call app_window_show within 2.5s
                let win_clone = main_win.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(2500));
                    let _ = win_clone.show();
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed => {
                    save_window_state(window);
                    let _ = stop_we_wallpaper();
                    window.app_handle().exit(0);
                }
                tauri::WindowEvent::Resized(_) if window.is_minimized().unwrap_or(false) => {
                    if let Some(wv) = window.app_handle().get_webview_window(window.label()) {
                        set_webview_background_mode(&wv, true);
                    }
                }
                tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. } => {
                    if let Some(wv) = window.app_handle().get_webview_window(window.label()) {
                        apply_ui_zoom(&wv);
                    }
                    if WEBVIEW_BACKGROUND_MODE.load(std::sync::atomic::Ordering::SeqCst) {
                        if let Some(wv) = window.app_handle().get_webview_window(window.label()) {
                            set_webview_background_mode(&wv, false);
                        }
                    }
                    #[cfg(target_os = "windows")]
                    {
                        if let Some(hwnd_val) = get_we_window_hwnd() {
                            let app_hwnd = window.hwnd().map(|h| h.0 as we_win32::HWND).unwrap_or(std::ptr::null_mut());
                            if !app_hwnd.is_null() {
                                let (scale, pan_x, pan_y) = get_we_transform();
                                update_we_geometry_internal(hwnd_val as we_win32::HWND, app_hwnd, scale, pan_x, pan_y);
                            }
                        }
                    }
                }
                tauri::WindowEvent::Focused(focused) => {
                    // also here, not only on close: the state survives a crash or a kill
                    if !*focused {
                        save_window_state(window);
                    }
                    if let Some(wv) = window.app_handle().get_webview_window(window.label()) {
                        set_webview_focused(&wv, *focused);
                    }
                    #[cfg(target_os = "windows")]
                    {
                        if let Some(hwnd_val) = get_we_window_hwnd() {
                            let app_hwnd = window.hwnd().map(|h| h.0 as we_win32::HWND).unwrap_or(std::ptr::null_mut());
                            if !app_hwnd.is_null() {
                                let is_min = unsafe { we_win32::IsIconic(app_hwnd) != 0 };
                                if is_min || *focused {
                                    thaw_we_scene_before_window_ops();
                                }
                                if is_min {
                                    unsafe {
                                        if let Some(u_val) = get_underlay_hwnd() {
                                            we_win32::ShowWindow(u_val as we_win32::HWND, we_win32::SW_HIDE);
                                        }
                                        we_win32::ShowWindow(hwnd_val as we_win32::HWND, we_win32::SW_HIDE);
                                    }
                                    *LAST_WE_GEOM.lock().unwrap_or_else(|e| e.into_inner()) = (0, 0, 0, 0);
                                } else if *focused {
                                    let (scale, pan_x, pan_y) = get_we_transform();
                                    update_we_geometry_internal(hwnd_val as we_win32::HWND, app_hwnd, scale, pan_x, pan_y);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            pick_folder,
            scan_directory,
            compare_instances,
            open_browser,
            toggle_mod_disabled,
            delete_mod_file,
            get_mod_details,
            launch_game,
            set_mod_user_category,
            reset_mod_user_category,
            get_user_categories,
            infer_mod_default_category,
            fetch_motionbgs_wallpapers,
            fetch_motionbgs_page,
            count_motionbgs_pages,
            download_wallpaper_file,
            check_wallpaper_file,
            get_steam_workshop_wallpapers,
            fetch_steam_workshop_online,
            fetch_steam_item_details,
            download_steam_workshop_item,
            launch_steamcmd_login,
            check_steam_item_installed,
            read_local_wallpaper_blob,
            get_all_instances,
            rename_instance,
            get_all_downloaded_wallpapers,
            pick_wallpaper_file,
            import_wallpaper_file,
            open_wallpapers_folder,
            delete_downloaded_wallpaper,
            sync_wallpaper_engine_desktop,
            app_window_minimize,
            app_window_toggle_maximize,
            app_window_close,
            app_window_start_dragging,
            app_window_show,
            find_wallpaper_engine_exe,
            get_we_preview_path,
            capture_we_snapshot,
            is_we_scene_rendering,
            freeze_we_scene,
            thaw_we_scene,
            rtss_status,
            rtss_exclude_launcher,
            play_we_wallpaper,
            stop_we_wallpaper,
            sync_we_window_rect,
            update_we_transform,
            apply_we_properties,
            get_we_wallpaper_properties,
            get_minecraft_block_textures,
            open_block_textures_folder,
            get_instance_log_status,
            create_new_instance,
            list_java_installations,
            pick_java_executable,
            download_java_runtime,
            get_system_memory_mb,
            get_account_skin,
            get_discovered_accounts,
            home_layout_load,
            home_layout_save,
            skins_service_status,
            get_instance_details,
            update_instance,
            console_tools::game_stats,
            console_tools::stop_game,
            share::share_room_create,
            share::share_room_get,
            share::share_room_update,
            share::share_room_close,
            share::share_room_put_chunk,
            share::share_room_get_chunk,
            share::share_box_get,
            share::share_box_send,
            share::share_box_ack,
            share::share_pack,
            share::share_read_chunk,
            share::share_discard,
            share::share_cleanup,
            share::share_recv_begin,
            share::share_recv_chunk,
            share::share_recv_abort,
            share::share_recv_finish,
            console_tools::list_instance_logs,
            console_tools::read_instance_log,
            prepare_server_skins,
            export_skins_for_hosting,
            manager::list_instance_packs,
            manager::set_instance_shader,
            manager::set_instance_resourcepacks,
            manager::instance_meta,
            manager::duplicate_instance,
            manager::trash_instance,
            manager::install_mod_version,
            manager::install_instance_file,
            manager::delete_instance_file,
            manager::transfer_instance_file,
            manager::list_instance_config_files,
            manager::read_instance_config,
            manager::write_instance_config,
            manager::list_instance_configs,
            manager::export_instance_config,
            manager::cf_fingerprint,
            manager::cf_match_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_steam_wallpapers() {
        let items = scan_steam_workshop_wallpapers_inner();
        println!("Found {} steam wallpapers", items.len());
        for item in &items {
            println!("- ID: {}, Title: {}, Video: {}", item.id, item.title, item.video_path);
            assert!(!item.video_path.is_empty(), "Video path must not be empty");
            assert!(std::path::Path::new(&item.video_path).exists(), "Video path must exist on disk");
        }
        assert!(items.len() >= 3, "Should have found at least the 3 downloaded test items");
    }
}



