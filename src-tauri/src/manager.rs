//! Gestor de Instancia: shaderpacks, resourcepacks, duplicar / enviar a la papelera, instalar otra
//! versión de un mod (Modrinth) y exportar la configuración de una instancia.

use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::instance_root_dir;
use crate::scanner::{parse_mod_file, ModInfo};

/// Folder the game runs in (Prism uses `minecraft` or `.minecraft`; Battly uses the instance root).
fn game_dir(root: &Path) -> PathBuf {
    for name in ["minecraft", ".minecraft"] {
        let p = root.join(name);
        if p.is_dir() {
            return p;
        }
    }
    if root.join("instance.json").is_file()
        || root.join("mods").is_dir()
        || root.join("shaderpacks").is_dir()
        || root.join("resourcepacks").is_dir()
    {
        return root.to_path_buf();
    }
    if root.to_string_lossy().contains("IkaLauncher") {
        return root.join(".minecraft");
    }
    root.join("minecraft")
}

fn read_kv(path: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .collect()
}

/// Rewrites (or appends) `key=value` lines of a properties file, keeping everything else.
fn write_kv(path: &Path, sep: char, pairs: &[(&str, String)]) -> Result<(), String> {
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let mut done = vec![false; pairs.len()];
    let mut out: Vec<String> = old
        .lines()
        .map(|l| {
            for (i, (k, v)) in pairs.iter().enumerate() {
                if l.split_once(sep).map(|(a, _)| a.trim() == *k).unwrap_or(false) {
                    done[i] = true;
                    return format!("{}{}{}", k, sep, v);
                }
            }
            l.to_string()
        })
        .collect();
    for (i, (k, v)) in pairs.iter().enumerate() {
        if !done[i] {
            out.push(format!("{}{}{}", k, sep, v));
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, out.join("\n") + "\n").map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct PackFile {
    name: String,
    size: u64,
    is_dir: bool,
    description: String,
    /// Path of the pack's picture (resourcepacks only): pack.png itself in a folder, a copy
    /// taken out of the .zip otherwise
    icon: Option<String>,
    /// SHA-1 of the .zip, to find it on Modrinth (versions); None for folders
    sha1: Option<String>,
}

#[derive(Serialize)]
pub struct InstancePacks {
    shaders: Vec<PackFile>,
    shader_active: Option<String>,
    /// "Iris" | "Oculus" | "OptiFine" when one is installed
    shader_loader: Option<String>,
    resourcepacks: Vec<PackFile>,
    /// Active resourcepacks, highest priority first
    resourcepacks_on: Vec<String>,
}

fn shader_loader(root: &Path) -> Option<&'static str> {
    let mods = game_dir(root).join("mods");
    let names: Vec<String> = std::fs::read_dir(mods)
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_lowercase()).filter(|n| n.ends_with(".jar")).collect())
        .unwrap_or_default();
    if names.iter().any(|n| n.contains("oculus")) {
        Some("Oculus")
    } else if names.iter().any(|n| n.contains("iris")) {
        Some("Iris")
    } else if names.iter().any(|n| n.contains("optifine")) {
        Some("OptiFine")
    } else {
        None
    }
}

fn shader_config(game: &Path, loader: &str) -> PathBuf {
    match loader {
        "Oculus" => game.join("config").join("oculus.properties"),
        "OptiFine" => game.join("optionsshaders.txt"),
        _ => game.join("config").join("iris.properties"),
    }
}

fn active_shader(game: &Path, loader: Option<&str>) -> Option<String> {
    let loader = loader?;
    let kv = read_kv(&shader_config(game, loader));
    let get = |k: &str| kv.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());
    if loader != "OptiFine" && get("enableShaders").as_deref() == Some("false") {
        return None;
    }
    let name = get("shaderPack")?;
    if name.is_empty() || name == "OFF" || name == "(internal)" {
        None
    } else {
        Some(name)
    }
}

/// Text of a pack.mcmeta description (plain string or text component).
fn text_of(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(a) => a.iter().map(text_of).collect(),
        serde_json::Value::Object(o) => {
            let mut s = o.get("text").map(text_of).unwrap_or_default();
            if let Some(extra) = o.get("extra") {
                s.push_str(&text_of(extra));
            }
            s
        }
        _ => String::new(),
    }
}

fn strip_formatting(s: &str) -> String {
    let mut out = String::new();
    let mut skip = false;
    for c in s.chars() {
        if skip {
            skip = false;
        } else if c == '§' {
            skip = true;
        } else {
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// Description and picture of a pack. The picture used to travel inside the answer as a data
/// URL (half a megabyte of text per instance, parsed by the interface every time): now it is a
/// file the interface loads like any other image.
fn pack_meta(path: &Path, is_dir: bool) -> (String, Option<String>) {
    const MAX_ICON: u64 = 512 * 1024;
    if is_dir {
        let png = path.join("pack.png");
        let icon = std::fs::metadata(&png).ok().filter(|m| m.is_file() && m.len() <= MAX_ICON).map(|_| png.to_string_lossy().to_string());
        let desc = pack_description(std::fs::read_to_string(path.join("pack.mcmeta")).ok());
        return (desc, icon);
    }
    let (mcmeta, png): (Option<String>, Option<Vec<u8>>) = {
        let mut mcmeta = None;
        let mut png = None;
        if let Ok(f) = std::fs::File::open(path) {
            if let Ok(mut z) = zip::ZipArchive::new(f) {
                if let Ok(mut e) = z.by_name("pack.mcmeta") {
                    let mut s = String::new();
                    if e.read_to_string(&mut s).is_ok() {
                        mcmeta = Some(s);
                    }
                }
                if let Ok(mut e) = z.by_name("pack.png") {
                    if e.size() <= MAX_ICON {
                        let mut b = Vec::new();
                        if e.read_to_end(&mut b).is_ok() {
                            png = Some(b);
                        }
                    }
                }
            }
        }
        (mcmeta, png)
    };
    let icon = png.and_then(|bytes| {
        let dir = crate::disk_cache::data_dir().join("pack_icons");
        std::fs::create_dir_all(&dir).ok()?;
        // one file per pack and per version of it
        let (size, modified) = crate::disk_cache::file_stamp(path).unwrap_or((0, 0));
        let mut key: u64 = 0xcbf29ce484222325;
        for b in path.to_string_lossy().bytes() {
            key = (key ^ b as u64).wrapping_mul(0x100000001b3);
        }
        let file = dir.join(format!("{:016x}_{:x}_{:x}.png", key, size, modified));
        if !file.exists() {
            std::fs::write(&file, bytes).ok()?;
        }
        Some(file.to_string_lossy().to_string())
    });
    (pack_description(mcmeta), icon)
}

fn pack_description(mcmeta: Option<String>) -> String {
    mcmeta
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s.trim_start_matches('\u{feff}')).ok())
        .and_then(|v| v.get("pack").and_then(|p| p.get("description")).map(text_of))
        .map(|s| strip_formatting(&s))
        .unwrap_or_default()
}

/// What is remembered of a pack's .zip while it does not change (see disk_cache): its hash and,
/// once it was needed, its description and picture.
#[derive(Serialize, serde::Deserialize, Clone, Default)]
struct CachedPack {
    sha1: Option<String>,
    meta: Option<(String, Option<String>)>,
}

static PACK_CACHE: crate::disk_cache::DiskCache<CachedPack> = crate::disk_cache::DiskCache::new("pack_cache");

fn hash_file_sha1(path: &Path) -> Option<String> {
    use sha1::Digest;
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = sha1::Sha1::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Some(h.finalize().iter().map(|b| format!("{:02x}", b)).collect())
}

fn list_packs(dir: &Path, with_meta: bool) -> Vec<PackFile> {
    let mut out: Vec<PackFile> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let p = e.path();
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') {
                        return None;
                    }
                    let is_dir = p.is_dir();
                    let name_low = name.to_lowercase();
                    let is_zip = name_low.ends_with(".zip") || name_low.ends_with(".zip.disabled");
                    let is_other_archive = name_low.ends_with(".7z") || name_low.ends_with(".rar");
                    if !is_dir && !is_zip && !is_other_archive {
                        return None;
                    }
                    if is_dir || is_other_archive {
                        let (description, icon) = if is_dir && with_meta { pack_meta(&p, true) } else { (String::new(), None) };
                        let size = if is_dir { 0 } else { crate::disk_cache::file_stamp(&p).map(|s| s.0).unwrap_or(0) };
                        return Some(PackFile { name, size, is_dir, description, icon, sha1: None });
                    }
                    // the manager lists the packs every time it opens: hashing every pack (tens
                    // of MB each) and opening every .zip again took CPU and disk on each opening
                    let stamp = crate::disk_cache::file_stamp(&p);
                    let mut known = stamp.and_then(|s| PACK_CACHE.get(&p, s)).unwrap_or_default();
                    let mut changed = false;
                    if known.sha1.is_none() {
                        known.sha1 = hash_file_sha1(&p);
                        changed = true;
                    }
                    // a picture that was cleaned from the data folder is taken out again
                    let stale_icon = matches!(&known.meta, Some((_, Some(icon))) if !Path::new(icon).exists());
                    if with_meta && (known.meta.is_none() || stale_icon) {
                        known.meta = Some(pack_meta(&p, false));
                        changed = true;
                    }
                    if let (true, Some(s)) = (changed, stamp) {
                        PACK_CACHE.put(&p, s, known.clone());
                    }
                    let (description, icon) = if with_meta { known.meta.unwrap_or_default() } else { (String::new(), None) };
                    Some(PackFile { name, size: stamp.map(|s| s.0).unwrap_or(0), is_dir, description, icon, sha1: known.sha1 })
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// `resourcePacks:[...]` of options.txt (Minecraft's order: last entry = highest priority).
fn options_resourcepacks(game: &Path) -> Vec<String> {
    std::fs::read_to_string(game.join("options.txt"))
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.strip_prefix("resourcePacks:"))
        .and_then(|v| serde_json::from_str::<Vec<String>>(v.trim()).ok())
        .unwrap_or_default()
}

#[tauri::command(async)]
pub fn list_instance_packs(instance_path: String) -> Result<InstancePacks, String> {
    let root = instance_root_dir(&instance_path)?;
    let game = game_dir(&root);
    let loader = shader_loader(&root);
    let mut resourcepacks = list_packs(&game.join("resourcepacks"), true);
    if resourcepacks.is_empty() {
        resourcepacks = list_packs(&root.join("resourcepacks"), true);
    }
    if resourcepacks.is_empty() {
        resourcepacks = list_packs(&game.join("texturepacks"), true);
    }
    let mut shaders = list_packs(&game.join("shaderpacks"), false);
    if shaders.is_empty() {
        shaders = list_packs(&root.join("shaderpacks"), false);
    }
    let mut on: Vec<String> = options_resourcepacks(&game)
        .into_iter()
        .map(|e| e.strip_prefix("file/").map(str::to_string).unwrap_or(e))
        .filter(|n| resourcepacks.iter().any(|p| &p.name == n))
        .collect();
    on.reverse();
    Ok(InstancePacks {
        shaders,
        shader_active: active_shader(&game, loader),
        shader_loader: loader.map(str::to_string),
        resourcepacks,
        resourcepacks_on: on,
    })
}

/// Names of the entries in the instance's config folder (top level only), to tell which mods have
/// a config file or folder.
#[tauri::command(async)]
pub fn list_instance_configs(instance_path: String) -> Result<Vec<String>, String> {
    let root = instance_root_dir(&instance_path)?;
    let dir = game_dir(&root).join("config");
    Ok(std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect())
        .unwrap_or_default())
}

/// A file of the instance's config folder.
#[derive(Serialize)]
pub struct ConfigFile {
    /// Path inside `config/`, with forward slashes
    path: String,
    size: u64,
    /// Last change, in ms since the epoch
    modified: u64,
}

/// The text of a config file and when it was last changed (to notice someone else changing it).
#[derive(Serialize)]
pub struct ConfigText {
    text: String,
    modified: u64,
}

const CONFIG_MAX_FILES: usize = 4000;
const CONFIG_MAX_DEPTH: u8 = 5;
/// Bigger files are not opened in the launcher's editor
const CONFIG_MAX_EDIT: u64 = 2 * 1024 * 1024;

fn modified_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn walk_config(dir: &Path, prefix: &str, depth: u8, out: &mut Vec<ConfigFile>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if out.len() >= CONFIG_MAX_FILES {
            return;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let rel = if prefix.is_empty() { name } else { format!("{}/{}", prefix, name) };
        let Ok(meta) = e.metadata() else { continue };
        if meta.is_dir() {
            if depth < CONFIG_MAX_DEPTH {
                walk_config(&e.path(), &rel, depth + 1, out);
            }
        } else if meta.is_file() {
            out.push(ConfigFile { path: rel, size: meta.len(), modified: modified_ms(&meta) });
        }
    }
}

/// Every file in the instance's config folder (a few levels deep), to list them by mod.
#[tauri::command(async)]
pub fn list_instance_config_files(instance_path: String) -> Result<Vec<ConfigFile>, String> {
    let root = instance_root_dir(&instance_path)?;
    let mut out = Vec::new();
    walk_config(&game_dir(&root).join("config"), "", 0, &mut out);
    out.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));
    Ok(out)
}

/// The file `rel` (forward slashes) of the instance's config folder; nothing outside that folder.
fn config_file_path(instance_path: &str, rel: &str) -> Result<PathBuf, String> {
    let bad = rel.is_empty() || rel.contains(['\\', ':']) || rel.split('/').any(|s| s.is_empty() || s == "." || s == "..");
    if bad {
        return Err("Ruta no válida".to_string());
    }
    let root = instance_root_dir(instance_path)?;
    let mut path = game_dir(&root).join("config");
    for part in rel.split('/') {
        path.push(part);
    }
    Ok(path)
}

/// Reads a config file to edit it in the launcher (text files up to 2 MB).
#[tauri::command(async)]
pub fn read_instance_config(instance_path: String, path: String) -> Result<ConfigText, String> {
    let file = config_file_path(&instance_path, &path)?;
    let meta = std::fs::metadata(&file).map_err(|_| "El archivo ya no existe en el disco.".to_string())?;
    if !meta.is_file() {
        return Err("No es un archivo.".to_string());
    }
    if meta.len() > CONFIG_MAX_EDIT {
        return Err("Es demasiado grande para editarlo aquí (más de 2 MB).".to_string());
    }
    let bytes = std::fs::read(&file).map_err(|e| e.to_string())?;
    if bytes.contains(&0) {
        return Err("No es un archivo de texto.".to_string());
    }
    let text = String::from_utf8(bytes).map_err(|_| "No es un archivo de texto (UTF-8).".to_string())?;
    Ok(ConfigText { text, modified: modified_ms(&meta) })
}

/// Saves an edited config file. With `expected_modified`, it refuses ("CHANGED") when the file was
/// changed on disk since it was read (the game rewrites its configs), so nothing is overwritten
/// unknowingly. Returns the file's new change time.
#[tauri::command(async)]
pub fn write_instance_config(instance_path: String, path: String, text: String, expected_modified: Option<u64>) -> Result<u64, String> {
    let file = config_file_path(&instance_path, &path)?;
    let meta = std::fs::metadata(&file).map_err(|_| "El archivo ya no existe en el disco.".to_string())?;
    if !meta.is_file() {
        return Err("No es un archivo.".to_string());
    }
    if let Some(expected) = expected_modified {
        if modified_ms(&meta) != expected {
            return Err("CHANGED".to_string());
        }
    }
    std::fs::write(&file, text.as_bytes()).map_err(|e| format!("No se pudo guardar: {}", e))?;
    Ok(std::fs::metadata(&file).map(|m| modified_ms(&m)).unwrap_or(0))
}

/// Activates a shaderpack (or turns shaders off with `None`) in the installed shader loader's config.
#[tauri::command(async)]
pub fn set_instance_shader(instance_path: String, shader: Option<String>) -> Result<(), String> {
    let root = instance_root_dir(&instance_path)?;
    let game = game_dir(&root);
    let loader = shader_loader(&root).ok_or("Esta instancia no tiene Iris, Oculus ni OptiFine")?;
    let cfg = shader_config(&game, loader);
    if loader == "OptiFine" {
        write_kv(&cfg, '=', &[("shaderPack", shader.unwrap_or_else(|| "OFF".to_string()))])
    } else {
        match shader {
            Some(name) => write_kv(&cfg, '=', &[("shaderPack", name), ("enableShaders", "true".to_string())]),
            None => write_kv(&cfg, '=', &[("enableShaders", "false".to_string())]),
        }
    }
}

/// Active resourcepacks, highest priority first. Built-in entries (vanilla, fabric, mod resources…)
/// stay as they were, below the packs.
#[tauri::command(async)]
pub fn set_instance_resourcepacks(instance_path: String, enabled: Vec<String>) -> Result<(), String> {
    let root = instance_root_dir(&instance_path)?;
    let game = game_dir(&root);
    let mut list: Vec<String> = options_resourcepacks(&game).into_iter().filter(|e| !e.starts_with("file/")).collect();
    if !list.iter().any(|e| e == "vanilla") {
        list.insert(0, "vanilla".to_string());
    }
    list.extend(enabled.iter().rev().map(|n| format!("file/{}", n)));
    let value = serde_json::to_string(&list).map_err(|e| e.to_string())?;
    write_kv(&game.join("options.txt"), ':', &[("resourcePacks", value)])
}

#[derive(Serialize)]
pub struct InstanceMeta {
    managed: bool,
    /// "Modrinth" | "CurseForge" | other launcher pack type
    pack_type: Option<String>,
    pack_name: Option<String>,
    notes: String,
}

#[tauri::command(async)]
pub fn instance_meta(instance_path: String) -> Result<InstanceMeta, String> {
    let root = instance_root_dir(&instance_path)?;
    let kv = read_kv(&root.join("instance.cfg"));
    let get = |k: &str| kv.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone()).filter(|v| !v.is_empty());
    let pack_type = get("ManagedPackType").map(|t| match t.to_lowercase().as_str() {
        "modrinth" => "Modrinth".to_string(),
        "flame" | "curseforge" => "CurseForge".to_string(),
        "technic" => "Technic".to_string(),
        "atlauncher" => "ATLauncher".to_string(),
        _ => t,
    });
    Ok(InstanceMeta {
        managed: get("ManagedPack").as_deref() == Some("true"),
        pack_type,
        pack_name: get("ManagedPackName"),
        notes: get("notes").map(|n| n.replace("\\n", "\n")).unwrap_or_default(),
    })
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let src = e.path();
        let dst = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// Copies the instance next to the original ("Nombre (copia)") and returns the new folder.
#[tauri::command(async)]
pub fn duplicate_instance(instance_path: String) -> Result<String, String> {
    let root = instance_root_dir(&instance_path)?;
    let parent = root.parent().ok_or("Ruta de instancia inválida")?;
    let folder = root.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    let mut n = 1;
    let target = loop {
        let name = if n == 1 { format!("{} (copia)", folder) } else { format!("{} (copia {})", folder, n) };
        let p = parent.join(&name);
        if !p.exists() {
            break p;
        }
        n += 1;
    };
    copy_dir(&root, &target).map_err(|e| format!("No se pudo copiar la instancia: {}", e))?;
    // logs of the original are not part of the copy
    let _ = std::fs::remove_dir_all(game_dir(&target).join("logs"));
    let _ = std::fs::remove_dir_all(game_dir(&target).join("crash-reports"));
    let kv = read_kv(&root.join("instance.cfg"));
    let name = kv.iter().find(|(k, _)| k == "name").map(|(_, v)| v.clone()).unwrap_or(folder);
    let copy_name = if n == 1 { format!("{} (copia)", name) } else { format!("{} (copia {})", name, n) };
    write_kv(&target.join("instance.cfg"), '=', &[("name", copy_name)])?;
    Ok(target.to_string_lossy().to_string())
}

/// Moves the instance to the launcher's recycle folder (recoverable), out of every instance list.
#[tauri::command(async)]
pub fn trash_instance(instance_path: String) -> Result<String, String> {
    let root = instance_root_dir(&instance_path)?;
    let trash = crate::mc::instances_root().parent().map(|p| p.join("papelera")).ok_or("No se encontró la carpeta del launcher")?;
    std::fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
    let folder = root.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let target = trash.join(format!("{}-{}", folder, stamp));
    if std::fs::rename(&root, &target).is_err() {
        // other drive: copy, then remove the original
        copy_dir(&root, &target).map_err(|e| format!("No se pudo mover la instancia: {}", e))?;
        std::fs::remove_dir_all(&root).map_err(|e| format!("Se copió a la papelera, pero no se pudo borrar el original: {}", e))?;
    }
    Ok(target.to_string_lossy().to_string())
}

/// Downloads a mod file from Modrinth into the mods folder and removes the file it replaces.
#[tauri::command(async)]
pub fn install_mod_version(mods_dir: String, url: String, filename: String, replace_path: Option<String>) -> Result<ModInfo, String> {
    if !filename.to_lowercase().ends_with(".jar") {
        return Err("Nombre de archivo inválido".to_string());
    }
    let dir = PathBuf::from(&mods_dir);
    if !dir.is_dir() {
        return Err("No existe la carpeta de mods".to_string());
    }
    let target = download_content_file(&dir, &url, &filename)?;
    if let Some(old) = replace_path {
        let old = PathBuf::from(old);
        if old != target && old.starts_with(&dir) {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(parse_mod_file(&target))
}

/// Downloads a Modrinth CDN file into `dir` (through a .part file, so a failed download leaves
/// nothing half written) and returns its final path.
fn download_content_file(dir: &Path, url: &str, filename: &str) -> Result<PathBuf, String> {
    // the file servers of Modrinth and CurseForge, nothing else
    const SOURCES: [&str; 3] = ["https://cdn.modrinth.com/", "https://edge.forgecdn.net/", "https://mediafilez.forgecdn.net/"];
    if !SOURCES.iter().any(|source| url.starts_with(source)) {
        return Err("Solo se descargan archivos de Modrinth o CurseForge".to_string());
    }
    if filename.is_empty() || filename.contains(['/', '\\']) || filename.starts_with('.') {
        return Err("Nombre de archivo inválido".to_string());
    }
    let target = dir.join(filename);
    let part = dir.join(format!("{}.part", filename));
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(300)).build();
    let resp = agent.get(url).set("User-Agent", "IkaLauncher/1.0").call().map_err(|e| format!("Descarga fallida: {}", e))?;
    let mut reader = resp.into_reader();
    let mut f = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    std::io::copy(&mut reader, &mut f).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        format!("Descarga interrumpida: {}", e)
    })?;
    f.flush().map_err(|e| e.to_string())?;
    drop(f);
    std::fs::rename(&part, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

/// Downloads new content (or another version of it) from Modrinth or CurseForge into the instance:
/// `folder` is "mods" (.jar), "shaderpacks" or "resourcepacks" (.zip). The folder is created if
/// the instance never had one. `replace_name` is the file of that folder it replaces.
/// Returns the name of the installed file.
#[tauri::command(async)]
pub fn install_instance_file(instance_path: String, folder: String, url: String, filename: String, replace_name: Option<String>) -> Result<String, String> {
    let ext = match folder.as_str() {
        "mods" => ".jar",
        "shaderpacks" | "resourcepacks" => ".zip",
        _ => return Err("Carpeta no válida".to_string()),
    };
    if !filename.to_lowercase().ends_with(ext) {
        return Err(format!("Solo se pueden instalar archivos {}", ext));
    }
    let root = instance_root_dir(&instance_path)?;
    let dir = game_dir(&root).join(&folder);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let target = download_content_file(&dir, &url, &filename)?;
    if let Some(old) = replace_name.filter(|n| !n.contains(['/', '\\']) && *n != filename) {
        let old = dir.join(old);
        if old.is_file() {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(filename))
}

/// Deletes a shaderpack, resourcepack, or config file of the instance.
#[tauri::command(async)]
pub fn delete_instance_file(instance_path: String, folder: String, name: String) -> Result<(), String> {
    let target = if folder == "config" {
        config_file_path(&instance_path, &name)?
    } else if matches!(folder.as_str(), "shaderpacks" | "resourcepacks") {
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
            return Err("Nombre no válido".to_string());
        }
        let root = instance_root_dir(&instance_path)?;
        let game = game_dir(&root);
        let mut p = game.join(&folder).join(&name);
        if !p.exists() {
            let alt = root.join(&folder).join(&name);
            if alt.exists() {
                p = alt;
            } else if folder == "resourcepacks" {
                let tex = game.join("texturepacks").join(&name);
                if tex.exists() {
                    p = tex;
                }
            }
        }
        p
    } else {
        return Err("Carpeta no válida".to_string());
    };
    let done = if target.is_dir() {
        std::fs::remove_dir_all(&target)
    } else if target.is_file() {
        std::fs::remove_file(&target)
    } else {
        return Err("El archivo ya no existe en el disco.".to_string());
    };
    done.map_err(|e| format!("Error al eliminar: {}", e))
}

/// Copies (`move_file = false`) or moves/cuts (`move_file = true`) a mod, resourcepack,
/// shaderpack, or config file from `src_instance_path` to `dest_instance_path`.
#[tauri::command(async)]
pub fn transfer_instance_file(
    src_instance_path: String,
    dest_instance_path: String,
    folder: String,
    name: String,
    move_file: bool,
) -> Result<(), String> {
    let src_root = instance_root_dir(&src_instance_path)?;
    let dest_root = instance_root_dir(&dest_instance_path)?;
    if src_root == dest_root {
        return Err("La instancia de destino es la misma que la de origen.".to_string());
    }

    let (src_target, dest_target) = if folder == "config" {
        let s = config_file_path(&src_instance_path, &name)?;
        let d = config_file_path(&dest_instance_path, &name)?;
        (s, d)
    } else if matches!(folder.as_str(), "mods" | "shaderpacks" | "resourcepacks") {
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
            return Err("Nombre de archivo no válido.".to_string());
        }
        let src_game = game_dir(&src_root);
        let mut s = src_game.join(&folder).join(&name);
        if !s.exists() {
            let alt = src_root.join(&folder).join(&name);
            if alt.exists() {
                s = alt;
            } else if folder == "resourcepacks" {
                let tex = src_game.join("texturepacks").join(&name);
                if tex.exists() {
                    s = tex;
                }
            }
        }
        let d = game_dir(&dest_root).join(&folder).join(&name);
        (s, d)
    } else {
        return Err("Carpeta no válida.".to_string());
    };

    if !src_target.exists() {
        return Err("El archivo ya no existe en el disco.".to_string());
    }
    if let Some(parent) = dest_target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("No se pudo crear la carpeta destino: {}", e))?;
    }

    if src_target.is_dir() {
        if dest_target.exists() {
            let _ = std::fs::remove_dir_all(&dest_target);
        }
        copy_dir(&src_target, &dest_target).map_err(|e| format!("Error al copiar carpeta: {}", e))?;
        if move_file {
            std::fs::remove_dir_all(&src_target).map_err(|e| format!("Se copió, pero no se pudo mover la carpeta original: {}", e))?;
        }
    } else {
        std::fs::copy(&src_target, &dest_target).map_err(|e| format!("Error al copiar el archivo: {}", e))?;
        if move_file {
            std::fs::remove_file(&src_target).map_err(|e| format!("Se copió, pero no se pudo mover el archivo original: {}", e))?;
        }
    }

    Ok(())
}

fn zip_dir(z: &mut zip::ZipWriter<std::fs::File>, dir: &Path, prefix: &str) -> Result<(), String> {
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for e in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let p = e.path();
        let name = format!("{}/{}", prefix, e.file_name().to_string_lossy());
        if p.is_dir() {
            zip_dir(z, &p, &name)?;
        } else if let Ok(bytes) = std::fs::read(&p) {
            z.start_file(name, opts).map_err(|e| e.to_string())?;
            z.write_all(&bytes).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Zips options, shader settings, servers, the config folder and the mod list into Downloads.
#[tauri::command(async)]
pub fn export_instance_config(instance_path: String) -> Result<String, String> {
    let root = instance_root_dir(&instance_path)?;
    let game = game_dir(&root);
    let folder = root.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| "instancia".to_string());
    let downloads = std::env::var("USERPROFILE").map(|h| PathBuf::from(h).join("Downloads")).unwrap_or_else(|_| root.clone());
    std::fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;
    let out = downloads.join(format!("{} - configuracion.zip", folder));
    let file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
    let mut z = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for name in ["options.txt", "optionsshaders.txt", "servers.dat"] {
        if let Ok(bytes) = std::fs::read(game.join(name)) {
            z.start_file(name, opts).map_err(|e| e.to_string())?;
            z.write_all(&bytes).map_err(|e| e.to_string())?;
        }
    }
    if game.join("config").is_dir() {
        zip_dir(&mut z, &game.join("config"), "config")?;
    }
    let mods: Vec<String> = std::fs::read_dir(game.join("mods"))
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".jar") || n.ends_with(".disabled")).collect())
        .unwrap_or_default();
    z.start_file("mods.txt", opts).map_err(|e| e.to_string())?;
    z.write_all(mods.join("\n").as_bytes()).map_err(|e| e.to_string())?;
    z.finish().map_err(|e| e.to_string())?;
    Ok(out.to_string_lossy().to_string())
}

/// CurseForge fingerprint of a file: murmur2 (seed 1) over its bytes without whitespace (9, 10, 13, 32).
/// Identifies a mod jar exactly on CurseForge, like the SHA1 does on Modrinth.
#[tauri::command(async)]
pub fn cf_fingerprint(file_path: String) -> Result<u32, String> {
    let bytes = std::fs::read(&file_path).map_err(|e| e.to_string())?;
    let data: Vec<u8> = bytes.into_iter().filter(|b| !matches!(b, 9 | 10 | 13 | 32)).collect();
    const M: u32 = 0x5bd1_e995;
    let mut h: u32 = 1 ^ (data.len() as u32);
    let mut chunks = data.chunks_exact(4);
    for c in &mut chunks {
        let mut k = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        k = k.wrapping_mul(M);
        k ^= k >> 24;
        k = k.wrapping_mul(M);
        h = h.wrapping_mul(M) ^ k;
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        for (i, b) in rem.iter().enumerate() {
            h ^= (*b as u32) << (8 * i);
        }
        h = h.wrapping_mul(M);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(M);
    h ^= h >> 15;
    Ok(h)
}

/// CurseForge mod id of a jar, matched exactly by its fingerprint. Done here because the webview
/// can't POST to the CurseForge API (CORS).
#[tauri::command(async)]
pub fn cf_match_file(file_path: String, api_key: String) -> Result<Option<u64>, String> {
    let fp = cf_fingerprint(file_path)?;
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(20)).build();
    let resp = agent
        .post("https://api.curseforge.com/v1/fingerprints/432")
        .set("x-api-key", &api_key)
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .send_string(&format!("{{\"fingerprints\":[{}]}}", fp))
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&resp.into_string().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(v.pointer("/data/exactMatches/0/id").and_then(|x| x.as_u64()))
}
