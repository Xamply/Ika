use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use rayon::prelude::*;
use sha1::{Sha1, Digest};

use crate::disk_cache::{file_stamp, DiskCache};
use crate::knowledge::{are_equivalents, canonical_category, find_known_mod, infer_category_and_tags};

/// Regular expressions are compiled once: a scan compiled some twenty per mod, and the
/// translation of each description one per phrase it knows.
pub fn cached_regex(pattern: &str) -> Result<Regex, regex::Error> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<HashMap<String, Regex>>> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    if let Some(re) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(pattern) {
        return Ok(re.clone());
    }
    let re = Regex::new(pattern)?;
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(pattern.to_string(), re.clone());
    Ok(re)
}

/// What a scan remembers of a mod's file (see disk_cache). The hash depends only on the file.
/// The rest also depends on the launcher (its knowledge base, how it names and classifies
/// mods): after the launcher changes (`build`) it is read from the file again, without hashing
/// the file again.
#[derive(Serialize, Deserialize, Clone)]
struct ScannedMod {
    sha1: String,
    build: u64,
    info: ModInfo,
}

static SCAN_CACHE: DiskCache<ScannedMod> = DiskCache::new("mod_scan_cache");

/// Loads the scan cache in the background at startup (a few MB of JSON): the first instance shown
/// is scanned right after, and that scan used to wait for the file.
pub fn warm_cache() {
    SCAN_CACHE.warm();
}

/// Identifies this build of the launcher: the size and date of its executable.
fn build_stamp() -> u64 {
    static STAMP: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *STAMP.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .and_then(|exe| file_stamp(&exe))
            .map(|(size, modified)| size ^ modified.rotate_left(17))
            .unwrap_or(0)
    })
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModInfo {
    pub filepath: String,
    pub filename: String,
    pub size_str: String,
    pub sha1: String,
    pub mod_id: String,
    pub canonical_id: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub loaders: Vec<String>,
    pub mc_versions: Vec<String>,
    pub clean_slug: String,
    pub category: String,
    pub tags: Vec<String>,
    pub is_api: bool,
    pub is_disabled: bool,
    pub declared_dependencies: Vec<String>,
    pub provides: Vec<String>,
    pub equivalent_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MatchResult {
    pub candidate_filepath: String,
    pub candidate_name: String,
    pub match_type: String, // "exact", "equivalent", "similar"
    pub score: f32,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EquivalentPair {
    pub mod1_name: String,
    pub mod1_file: String,
    pub mod2_name: String,
    pub mod2_file: String,
    pub category: String,
    pub note: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CategorySummary {
    pub category: String,
    pub missing_count: usize,
    pub covered_count: usize,
    pub missing_mods: Vec<ModInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DependencyIssue {
    pub missing_dep_id: String,
    pub missing_dep_name: String,
    pub required_by: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ComparisonOutput {
    pub matches_1_to_2: HashMap<String, MatchResult>,
    pub matches_2_to_1: HashMap<String, MatchResult>,
    pub missing_in_2: Vec<ModInfo>,
    pub equivalents_1_to_2: Vec<EquivalentPair>,
    pub category_summaries: Vec<CategorySummary>,
    pub dependency_issues_inst2: Vec<DependencyIssue>,
    pub common_count: usize,
    pub missing_count: usize,
    pub equivalent_count: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ScanOutput {
    pub resolved_dir: String,
    pub mods: Vec<ModInfo>,
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn clean_slug_from_filename(
    name: &str,
    loaders: &mut Vec<String>,
    mc_versions: &mut Vec<String>,
) -> String {
    let mut clean = name.to_string();
    for ext in &[".jar.disabled", ".jar", ".disabled"] {
        if clean.to_lowercase().ends_with(ext) {
            clean.truncate(clean.len() - ext.len());
            break;
        }
    }

    // Strip common creator / publisher prefixes
    if let Ok(re) = cached_regex(r"(?i)\[let'?s\s*do\]\s*") {
        clean = re.replace_all(&clean, "").to_string();
    }
    if let Ok(re) = cached_regex(r"(?i)\bletsdo[-_]") {
        clean = re.replace_all(&clean, "").to_string();
    }
    if let Ok(re) = cached_regex(r"(?i)\byungs?[-_]") {
        clean = re.replace_all(&clean, "").to_string();
    }
    if let Ok(re) = cached_regex(r"(?i)\bmacaws?[-_]") {
        clean = re.replace_all(&clean, "").to_string();
    }
    if let Ok(re) = cached_regex(r"(?i)\bserilum[-_]") {
        clean = re.replace_all(&clean, "").to_string();
    }

    // Detect loaders
    for loader in &["fabric", "neoforge", "forge", "quilt", "liteloader", "rift"] {
        let pattern = format!(r"(?i)\b{}\b", loader);
        if let Ok(re) = cached_regex(&pattern) {
            if re.is_match(&clean) && !loaders.iter().any(|l| l.eq_ignore_ascii_case(loader)) {
                loaders.push(loader.to_string());
            }
        }
    }

    // Detect MC versions
    if let Ok(re) = cached_regex(r"(?i)(?:mc|minecraft)?[-_+]?([12]\.\d+(?:\.\d+)?(?:-(?:rc|pre)\d+)?)") {
        for cap in re.captures_iter(&clean) {
            if let Some(m) = cap.get(1) {
                let ver = m.as_str().to_string();
                if !mc_versions.contains(&ver) {
                    mc_versions.push(ver);
                }
            }
        }
    }

    // Strip MC versions
    if let Ok(re) = cached_regex(r"(?i)[-_+.]?(?:mc|minecraft)?[-_+]?[12]\.\d+(?:\.\d+)?(?:-(?:rc|pre)\d+)?") {
        clean = re.replace_all(&clean, "").to_string();
    }
    // Strip loader names
    if let Ok(re) = cached_regex(r"(?i)[-_+.]?(fabric|neoforge|forge|quilt|liteloader|rift)") {
        clean = re.replace_all(&clean, "").to_string();
    }
    // Strip trailing version patterns
    if let Ok(re) = cached_regex(r"[-_+.][vV]?\d+[\d\.\-_+a-zA-Z]*$") {
        clean = re.replace(&clean, "").to_string();
    }

    // Lowercase alphanumeric
    let slug: String = clean
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase();

    if slug.is_empty() {
        name.split('.').next().unwrap_or(name).to_lowercase()
    } else {
        slug
    }
}

/// The mod in that file: remembered while the file does not change, read from it otherwise.
pub fn parse_mod_file(path: &Path) -> ModInfo {
    let Some(stamp) = file_stamp(path) else {
        return read_mod_file(path, None);
    };
    let build = build_stamp();
    let cached = SCAN_CACHE.get(path, stamp);
    if let Some(c) = &cached {
        if c.build == build {
            return c.info.clone();
        }
    }
    let info = read_mod_file(path, cached.map(|c| c.sha1));
    SCAN_CACHE.put(path, stamp, ScannedMod { sha1: info.sha1.clone(), build, info: info.clone() });
    info
}

/// Reads the mod from its file. `known_sha1`: its hash, when the file was already hashed.
fn read_mod_file(path: &Path, known_sha1: Option<String>) -> ModInfo {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let size_str = format_size(size_bytes);

    // Compute SHA-1
    let sha1 = known_sha1.unwrap_or_else(|| {
        let mut sha1 = String::new();
        if let Ok(mut f) = File::open(path) {
            let mut hasher = Sha1::new();
            let mut buffer = vec![0u8; 1 << 18];
            while let Ok(n) = f.read(&mut buffer) {
                if n == 0 { break; }
                hasher.update(&buffer[..n]);
            }
            sha1 = format!("{:x}", hasher.finalize());
        }
        sha1
    });

    let mut loaders = Vec::new();
    let mut mc_versions = Vec::new();
    let clean_slug = clean_slug_from_filename(&filename, &mut loaders, &mut mc_versions);

    let mut mod_id = String::new();
    let mut display_name = String::new();
    let mut description = String::new();
    let mut version = String::new();
    let mut declared_dependencies = Vec::new();
    let mut provides = Vec::new();

    // Inspect inside zip archive
    if let Ok(file) = File::open(path) {
        if let Ok(mut archive) = zip::ZipArchive::new(file) {
            // 1. Fabric mod
            if let Ok(mut zfile) = archive.by_name("fabric.mod.json") {
                if !loaders.iter().any(|l| l == "fabric") {
                    loaders.push("fabric".to_string());
                }
                let mut content = String::new();
                if zfile.read_to_string(&mut content).is_ok() {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(id) = v.get("id").and_then(|s| s.as_str()) {
                            mod_id = id.to_string();
                        }
                        if let Some(name) = v.get("name").and_then(|s| s.as_str()) {
                            display_name = name.to_string();
                        }
                        if let Some(ver) = v.get("version").and_then(|s| s.as_str()) {
                            version = ver.to_string();
                        }
                        if let Some(desc) = v.get("description").and_then(|s| s.as_str()) {
                            description = desc.trim().to_string();
                        }
                        // Parse provides (e.g. fabric-api provides 'fabric')
                        if let Some(prov_arr) = v.get("provides").and_then(|p| p.as_array()) {
                            for p in prov_arr {
                                if let Some(p_str) = p.as_str() {
                                    provides.push(p_str.to_string());
                                }
                            }
                        }
                        // Parse depends (ONLY real mods, not minecraft/fabricloader)
                        if let Some(deps) = v.get("depends").and_then(|d| d.as_object()) {
                            for k in deps.keys() {
                                if k != "minecraft" && k != "fabricloader" && k != "java" && k != "quilt_loader" {
                                    declared_dependencies.push(k.clone());
                                }
                            }
                        }
                    }
                }
            }

            // 2. Quilt mod
            if mod_id.is_empty() {
                if let Ok(mut zfile) = archive.by_name("quilt.mod.json") {
                    if !loaders.iter().any(|l| l == "quilt") {
                        loaders.push("quilt".to_string());
                    }
                    let mut content = String::new();
                    if zfile.read_to_string(&mut content).is_ok() {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(ql) = v.get("quilt_loader") {
                                if let Some(id) = ql.get("id").and_then(|s| s.as_str()) {
                                    mod_id = id.to_string();
                                }
                                if let Some(deps) = ql.get("depends").and_then(|d| d.as_array()) {
                                    for item in deps {
                                        if let Some(s) = item.get("id").and_then(|s| s.as_str()) {
                                            if s != "minecraft" && s != "quilt_loader" && s != "java" {
                                                declared_dependencies.push(s.to_string());
                                            }
                                        }
                                    }
                                }
                                if let Some(provs) = ql.get("provides").and_then(|p| p.as_array()) {
                                    for p in provs {
                                        if let Some(s) = p.get("id").and_then(|s| s.as_str()) {
                                            provides.push(s.to_string());
                                        }
                                    }
                                }
                            }
                            if let Some(meta) = v.get("metadata") {
                                if let Some(name) = meta.get("name").and_then(|s| s.as_str()) {
                                    display_name = name.to_string();
                                }
                                if let Some(ver) = meta.get("version").and_then(|s| s.as_str()) {
                                    version = ver.to_string();
                                }
                                if let Some(desc) = meta.get("description").and_then(|s| s.as_str()) {
                                    description = desc.trim().to_string();
                                }
                            }
                        }
                    }
                }
            }

            // 3. Forge / NeoForge mods.toml
            let mut toml_name = None;
            if archive.by_name("META-INF/neoforge.mods.toml").is_ok() {
                toml_name = Some("META-INF/neoforge.mods.toml");
                if !loaders.iter().any(|l| l == "neoforge") {
                    loaders.push("neoforge".to_string());
                }
            } else if archive.by_name("META-INF/mods.toml").is_ok() {
                toml_name = Some("META-INF/mods.toml");
                if !loaders.iter().any(|l| l == "forge") {
                    loaders.push("forge".to_string());
                }
            }

            if let Some(t_path) = toml_name {
                if let Ok(mut zfile) = archive.by_name(t_path) {
                    let mut content = String::new();
                    if zfile.read_to_string(&mut content).is_ok() {
                        if mod_id.is_empty() || mod_id == "forge" || mod_id == "minecraft" || mod_id == "neoforge" {
                            // First, attempt to extract specifically from under the [[mods]] section
                            if let Some(mods_part) = content.split("[[mods]]").nth(1) {
                                if let Ok(re) = cached_regex(r#"modId\s*=\s*(?:"([^"\r\n]+)"|'([^'\r\n]+)')"#) {
                                    if let Some(cap) = re.captures(mods_part) {
                                        if let Some(m) = cap.get(1).or_else(|| cap.get(2)) {
                                            let s = m.as_str().trim();
                                            if s != "forge" && s != "minecraft" && s != "neoforge" && s != "java" {
                                                mod_id = s.to_string();
                                            }
                                        }
                                    }
                                }
                            }
                            // Fallback if not found inside [[mods]]
                            if mod_id.is_empty() || mod_id == "forge" || mod_id == "minecraft" || mod_id == "neoforge" {
                                if let Ok(re) = cached_regex(r#"modId\s*=\s*(?:"([^"\r\n]+)"|'([^'\r\n]+)')"#) {
                                    for cap in re.captures_iter(&content) {
                                        if let Some(m) = cap.get(1).or_else(|| cap.get(2)) {
                                            let s = m.as_str().trim();
                                            if s != "forge" && s != "minecraft" && s != "neoforge" && s != "java" {
                                                mod_id = s.to_string();
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if display_name.is_empty() {
                            if let Ok(re) = cached_regex(r#"displayName\s*=\s*(?:"([^"\r\n]+)"|'([^'\r\n]+)')"#) {
                                if let Some(cap) = re.captures(&content) {
                                    if let Some(m) = cap.get(1).or_else(|| cap.get(2)) {
                                        display_name = m.as_str().to_string();
                                    }
                                }
                            }
                        }
                        if version.is_empty() {
                            if let Ok(re) = cached_regex(r#"version\s*=\s*(?:"([^"\r\n]+)"|'([^'\r\n]+)')"#) {
                                if let Some(cap) = re.captures(&content) {
                                    if let Some(m) = cap.get(1).or_else(|| cap.get(2)) {
                                        version = m.as_str().to_string();
                                    }
                                }
                            }
                        }
                        if description.is_empty() {
                            if let Ok(re) = cached_regex(r#"(?s)description\s*=\s*['"]{1,3}(.*?)['"]{1,3}"#) {
                                if let Some(cap) = re.captures(&content) {
                                    if let Some(m) = cap.get(1) {
                                        description = m.as_str().trim().replace('\n', " ").to_string();
                                    }
                                }
                            }
                        }

                        // Extract declared dependencies ONLY IF MANDATORY = TRUE
                        // In Forge/NeoForge mods.toml:
                        // [[dependencies.modname]]
                        // modId = "..."
                        // mandatory = true
                        // Split by [[dependencies.
                        let parts: Vec<&str> = content.split("[[dependencies.").collect();
                        for part in parts.iter().skip(1) {
                            let block = part.split("[[").next().unwrap_or(part);
                            
                            // Check if mandatory is false or optional
                            let is_optional = block.contains("mandatory=false")
                                || block.contains("mandatory = false")
                                || block.contains("type=\"optional\"")
                                || block.contains("type = \"optional\"");

                            if is_optional {
                                continue;
                            }

                            // Extract modId
                            if let Ok(re_id) = cached_regex(r#"modId\s*=\s*(?:"([^"\r\n]+)"|'([^'\r\n]+)')"#) {
                                if let Some(cap) = re_id.captures(block) {
                                    if let Some(m) = cap.get(1).or_else(|| cap.get(2)) {
                                        let id = m.as_str().to_string();
                                        if id != "minecraft" && id != "forge" && id != "neoforge" && id != "fml" && id != "java" {
                                            if !declared_dependencies.contains(&id) {
                                                declared_dependencies.push(id);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 4. mcmod.info (Legacy Forge)
            if mod_id.is_empty() {
                if let Ok(mut zfile) = archive.by_name("mcmod.info") {
                    let mut content = String::new();
                    if zfile.read_to_string(&mut content).is_ok() {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(arr) = v.as_array() {
                                if let Some(first) = arr.first() {
                                    if let Some(id) = first.get("modid").and_then(|s| s.as_str()) {
                                        mod_id = id.to_string();
                                    }
                                    if let Some(name) = first.get("name").and_then(|s| s.as_str()) {
                                        display_name = name.to_string();
                                    }
                                    if let Some(desc) = first.get("description").and_then(|s| s.as_str()) {
                                        description = desc.trim().to_string();
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Detect Architectury API usage (injected classes, common json, or specific mods like Super Resolution & Uncrafting Table)
            let mut uses_architectury = false;
            let fn_lower = filename.to_lowercase();
            for name in archive.file_names() {
                if name.contains("architectury_inject")
                    || name == "architectury.common.json"
                    || name.contains("dev/architectury")
                    || (name.contains("AboutInfoPage.class") && fn_lower.contains("resolution"))
                {
                    uses_architectury = true;
                    break;
                }
            }
            if (uses_architectury || fn_lower.contains("uncrafting") || fn_lower.contains("super_resolution") || fn_lower.contains("superresolution"))
                && mod_id != "architectury"
                && !declared_dependencies.contains(&"architectury".to_string())
            {
                declared_dependencies.push("architectury".to_string());
            }

            // Super Resolution depends on Cloth Config API
            if (fn_lower.contains("super_resolution") || fn_lower.contains("superresolution") || fn_lower.contains("super-resolution"))
                && !declared_dependencies.contains(&"cloth_config".to_string())
                && !declared_dependencies.contains(&"cloth-config".to_string())
            {
                declared_dependencies.push("cloth_config".to_string());
            }
        }
    }

    // Safety check for Let's Do mods or incomplete displayName
    if display_name == "[Let" || (display_name.starts_with("[Let") && !display_name.contains(']')) {
        let mut base_name = filename.clone();
        for ext in &[".jar.disabled", ".jar", ".disabled"] {
            if base_name.to_lowercase().ends_with(ext) {
                base_name.truncate(base_name.len() - ext.len());
                break;
            }
        }
        let clean_part = base_name.replace("letsdo-", "").replace("letsdo_", "").replace("letsdo", "");
        let clean_part = clean_part.split('-').next().unwrap_or(&clean_part);
        display_name = format!("[Let's Do] {}", titlecase(&clean_part.replace('_', " ").trim()));
    }

    if display_name.is_empty() {
        let mut base = filename.clone();
        for ext in &[".jar.disabled", ".jar", ".disabled"] {
            if base.to_lowercase().ends_with(ext) {
                base.truncate(base.len() - ext.len());
                break;
            }
        }
        let is_letsdo = filename.to_lowercase().contains("letsdo") || filename.to_lowercase().contains("let's do");
        if is_letsdo {
            let mut clean_sub = base.replace("letsdo-", "").replace("letsdo_", "").replace("letsdo", "");
            if let Ok(re) = cached_regex(r"(?i)[-_+.]?(?:mc|minecraft)?[-_+]?[12]\.\d+(?:\.\d+)?(?:-(?:rc|pre)\d+)?") {
                clean_sub = re.replace_all(&clean_sub, "").to_string();
            }
            if let Ok(re) = cached_regex(r"(?i)[-_+.]?(fabric|neoforge|forge|quilt|liteloader|rift)") {
                clean_sub = re.replace_all(&clean_sub, "").to_string();
            }
            if let Ok(re) = cached_regex(r"[-_+.][vV]?\d+[\d\.\-_+a-zA-Z]*$") {
                clean_sub = re.replace(&clean_sub, "").to_string();
            }
            clean_sub = clean_sub.replace('-', " ").replace('_', " ").trim().to_string();
            display_name = format!("[Let's Do] {}", titlecase(&clean_sub));
        } else {
            if let Ok(re) = cached_regex(r"(?i)[-_+.]?(?:mc|minecraft)?[-_+]?[12]\.\d+(?:\.\d+)?(?:-(?:rc|pre)\d+)?") {
                base = re.replace_all(&base, "").to_string();
            }
            if let Ok(re) = cached_regex(r"(?i)[-_+.]?(fabric|neoforge|forge|quilt|liteloader|rift)") {
                base = re.replace_all(&base, "").to_string();
            }
            if let Ok(re) = cached_regex(r"[-_+.][vV]?\d+[\d\.\-_+a-zA-Z]*$") {
                base = re.replace(&base, "").to_string();
            }
            base = base.replace('-', " ").replace('_', " ").trim().to_string();
            display_name = if !base.is_empty() {
                titlecase(&base)
            } else {
                filename.clone()
            };
        }
    }

    if mod_id.is_empty() {
        mod_id = clean_slug.clone();
    }

    // Match with knowledge base
    let known = find_known_mod(&clean_slug, &mod_id);

    let canonical_id;
    let category;
    let tags;
    let is_api;
    let final_description;
    let equivalent_ids;

    // Clean trailing " mod", " Mod", " (mod)" from display_name
    if let Ok(re) = cached_regex(r"(?i)[\s\-_]+(?:\(mod\)|mod)$") {
        display_name = re.replace(&display_name, "").to_string();
    }

    if let Some(k) = known {
        canonical_id = k.id.to_string();
        category = canonical_category(k.category).to_string();
        tags = k.tags.iter().map(|s| s.to_string()).collect();
        is_api = k.is_api;
        if !k.name.is_empty() {
            display_name = k.name.to_string();
        }
        final_description = if !k.description.is_empty() {
            k.description.to_string()
        } else if !description.is_empty() {
            crate::knowledge::translate_to_spanish(&description)
        } else {
            "Mod para Minecraft.".to_string()
        };
        equivalent_ids = k.equivalents.iter().map(|s| s.to_string()).collect();
    } else {
        canonical_id = mod_id.clone();
        let (inferred_cat, inferred_tags, inferred_api) = infer_category_and_tags(&display_name, &description, &clean_slug);
        category = canonical_category(inferred_cat).to_string();
        tags = inferred_tags;
        is_api = inferred_api;
        final_description = if !description.is_empty() {
            crate::knowledge::translate_to_spanish(&description)
        } else {
            format!("Mod de {}.", category.to_lowercase())
        };
        equivalent_ids = Vec::new();
    }

    let is_disabled = filename.to_lowercase().ends_with(".disabled");

    ModInfo {
        filepath: path.to_string_lossy().to_string(),
        filename,
        size_str,
        sha1,
        mod_id,
        canonical_id,
        display_name,
        description: final_description,
        version,
        loaders,
        mc_versions,
        clean_slug,
        category,
        tags,
        is_api,
        is_disabled,
        declared_dependencies,
        provides,
        equivalent_ids,
    }
}

fn titlecase(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in s.chars() {
        if c.is_whitespace() {
            capitalize_next = true;
            result.push(c);
        } else if capitalize_next {
            result.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

pub fn scan_mods_in_dir(dir_path: &str) -> ScanOutput {
    let base_path = PathBuf::from(dir_path);
    let mut target_path = base_path.clone();

    let sub_mods = base_path.join("mods");
    if sub_mods.is_dir() {
        target_path = sub_mods;
    }

    if !target_path.exists() {
        return ScanOutput {
            resolved_dir: dir_path.to_string(),
            mods: Vec::new(),
        };
    }

    let mut jar_paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&target_path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                if name.ends_with(".jar") || name.ends_with(".jar.disabled") {
                    jar_paths.push(p);
                }
            }
        }
    }

    // the files that are no longer in the folder are forgotten
    let seen: HashSet<String> = jar_paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
    SCAN_CACHE.forget_missing(&target_path, &seen);

    // Parallel extraction with Rayon
    let mut mods: Vec<ModInfo> = jar_paths
        .into_par_iter()
        .map(|p| parse_mod_file(&p))
        .collect();

    apply_user_categories(&mut mods);

    // Sort alphabetically by display name
    mods.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));

    ScanOutput {
        resolved_dir: target_path.to_string_lossy().to_string(),
        mods,
    }
}

fn normalize_alphanumeric(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase()
}

fn levenshtein_similarity(s1: &str, s2: &str) -> f32 {
    if s1 == s2 {
        return 1.0;
    }
    if s1.is_empty() || s2.is_empty() {
        return 0.0;
    }

    let len1 = s1.chars().count();
    let len2 = s2.chars().count();
    let mut v0: Vec<usize> = (0..=len2).collect();
    let mut v1: Vec<usize> = vec![0; len2 + 1];

    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();

    for i in 0..len1 {
        v1[0] = i + 1;
        for j in 0..len2 {
            let cost = if s1_chars[i] == s2_chars[j] { 0 } else { 1 };
            v1[j + 1] = (v1[j] + 1).min(v0[j + 1] + 1).min(v0[j] + cost);
        }
        v0.copy_from_slice(&v1);
    }

    let max_len = len1.max(len2) as f32;
    let dist = v0[len2] as f32;
    (1.0 - dist / max_len).max(0.0)
}

/// Normalized names of a mod, computed once per mod instead of once per compared pair.
struct NormNames {
    id: String,
    canon: String,
    disp: String,
}

impl NormNames {
    fn of(m: &ModInfo) -> Self {
        NormNames {
            id: normalize_alphanumeric(&m.mod_id),
            canon: normalize_alphanumeric(&m.canonical_id),
            disp: normalize_alphanumeric(&m.display_name),
        }
    }
}

#[allow(dead_code)]
pub fn calculate_similarity(a: &ModInfo, b: &ModInfo) -> (f32, String, &'static str) {
    similarity(a, &NormNames::of(a), b, &NormNames::of(b))
}

fn similarity(a: &ModInfo, na: &NormNames, b: &ModInfo, nb: &NormNames) -> (f32, String, &'static str) {
    let (norm_id_a, norm_id_b) = (&na.id, &nb.id);
    let (norm_canon_a, norm_canon_b) = (&na.canon, &nb.canon);

    // 1. Mod ID match or Canonical ID match
    if !norm_id_a.is_empty() && norm_id_a == norm_id_b {
        return (1.0, format!("ID identico ('{}')", a.mod_id), "exact");
    }
    if !norm_canon_a.is_empty() && norm_canon_a == norm_canon_b {
        return (1.0, format!("Identidad canónica ('{}')", a.canonical_id), "exact");
    }

    // 2. Inter-loader Equivalents Check
    if are_equivalents(&a.canonical_id, &b.canonical_id) || are_equivalents(&a.mod_id, &b.mod_id) {
        return (0.97, format!("Equivalente funcional entre loaders ({} ≈ {})", a.display_name, b.display_name), "equivalent");
    }

    // 3. Exact clean slug match
    if !a.clean_slug.is_empty() && a.clean_slug == b.clean_slug {
        return (0.95, format!("Nombre base identico ('{}')", a.clean_slug), "exact");
    }

    // 4. Exact display name match
    let (norm_disp_a, norm_disp_b) = (&na.disp, &nb.disp);
    if !norm_disp_a.is_empty() && norm_disp_a == norm_disp_b {
        return (0.94, format!("Nombre visible identico ('{}')", a.display_name), "exact");
    }

    // 5. Strict Substring containment (both must be >= 5 chars, length ratio >= 0.78)
    if a.clean_slug.len() >= 5 && b.clean_slug.len() >= 5 {
        if a.clean_slug.contains(&b.clean_slug) || b.clean_slug.contains(&a.clean_slug) {
            let min_len = a.clean_slug.len().min(b.clean_slug.len()) as f32;
            let max_len = a.clean_slug.len().max(b.clean_slug.len()) as f32;
            let ratio = min_len / max_len;
            if ratio >= 0.78 {
                return (0.90 * ratio, format!("Coincidencia parcial ({}%)", (90.0 * ratio) as u32), "similar");
            }
        }
    }

    // 6. Levenshtein edit distance similarity (strict threshold >= 0.82)
    let sim_slug = levenshtein_similarity(&a.clean_slug, &b.clean_slug);
    let sim_disp = levenshtein_similarity(norm_disp_a, norm_disp_b);
    let sim_id = levenshtein_similarity(norm_id_a, norm_id_b);

    let best = sim_slug.max(sim_disp).max(sim_id);
    if best >= 0.82 {
        (best, format!("Similitud de texto ({}%)", (best * 100.0) as u32), "similar")
    } else {
        (best, "Sin coincidencia".to_string(), "none")
    }
}

pub fn compare(mods1: &[ModInfo], mods2: &[ModInfo]) -> ComparisonOutput {
    let mut matches_1_to_2 = HashMap::new();
    let mut matches_2_to_1 = HashMap::new();
    let mut missing_in_2 = Vec::new();
    let mut equivalents_1_to_2 = Vec::new();

    let mut matched_in_2_paths: HashSet<String> = HashSet::new();

    let norms1: Vec<NormNames> = mods1.iter().map(NormNames::of).collect();
    let norms2: Vec<NormNames> = mods2.iter().map(NormNames::of).collect();
    // Best candidate of each mod in the other list (first one with the highest score,
    // exactly as the sequential loop picked it), computed on all cores.
    let best_match = |m: &ModInfo, nm: &NormNames, others: &'_ [ModInfo], others_n: &[NormNames]| {
        let mut best: (Option<usize>, f32, String, &'static str) = (None, 0.0, String::new(), "none");
        for (j, o) in others.iter().enumerate() {
            let (score, reason, mtype) = similarity(m, nm, o, &others_n[j]);
            if score > best.1 {
                best = (Some(j), score, reason, mtype);
            }
        }
        best
    };
    let best_1_to_2: Vec<_> = {
        use rayon::prelude::*;
        mods1.par_iter().zip(norms1.par_iter()).map(|(m, n)| best_match(m, n, mods2, &norms2)).collect()
    };
    let best_2_to_1: Vec<_> = {
        use rayon::prelude::*;
        mods2.par_iter().zip(norms2.par_iter()).map(|(m, n)| best_match(m, n, mods1, &norms1)).collect()
    };

    for (m1, (best_idx, best_score, best_reason, best_type)) in mods1.iter().zip(best_1_to_2) {
        let best_cand = best_idx.map(|j| &mods2[j]);

        if let Some(cand) = best_cand {
            if best_score >= 0.78 {
                matches_1_to_2.insert(
                    m1.filepath.clone(),
                    MatchResult {
                        candidate_filepath: cand.filepath.clone(),
                        candidate_name: cand.display_name.clone(),
                        match_type: best_type.to_string(),
                        score: best_score,
                        reason: best_reason.clone(),
                    },
                );
                matched_in_2_paths.insert(cand.filepath.clone());

                if best_type == "equivalent" {
                    equivalents_1_to_2.push(EquivalentPair {
                        mod1_name: m1.display_name.clone(),
                        mod1_file: m1.filename.clone(),
                        mod2_name: cand.display_name.clone(),
                        mod2_file: cand.filename.clone(),
                        category: m1.category.clone(),
                        note: best_reason,
                    });
                }
            } else {
                missing_in_2.push(m1.clone());
            }
        } else {
            missing_in_2.push(m1.clone());
        }
    }

    for (m2, (best_idx, best_score, best_reason, best_type)) in mods2.iter().zip(best_2_to_1) {
        let best_cand = best_idx.map(|j| &mods1[j]);

        if let Some(cand) = best_cand {
            if best_score >= 0.78 {
                matches_2_to_1.insert(
                    m2.filepath.clone(),
                    MatchResult {
                        candidate_filepath: cand.filepath.clone(),
                        candidate_name: cand.display_name.clone(),
                        match_type: best_type.to_string(),
                        score: best_score,
                        reason: best_reason,
                    },
                );
            }
        }
    }

    // Category Summaries for Missing Mods
    let categories_order = [
        "Rendimiento",
        "Graficos",
        "Mundo",
        "Estructuras",
        "Calidad de vida",
        "UI",
        "Contenido",
        "Combate",
        "Construccion",
        "Audio",
        "Dependencias",
        "Utilidades",
    ];

    let mut cat_map: HashMap<String, Vec<ModInfo>> = HashMap::new();
    for m in &missing_in_2 {
        cat_map.entry(m.category.clone()).or_default().push(m.clone());
    }

    let mut category_summaries = Vec::new();
    for cat in &categories_order {
        let cat_str = cat.to_string();
        let missing_mods = cat_map.remove(&cat_str).unwrap_or_default();
        let missing_count = missing_mods.len();
        let covered_count = mods1.iter().filter(|m| m.category == cat_str && matches_1_to_2.contains_key(&m.filepath)).count();

        if missing_count > 0 || covered_count > 0 {
            category_summaries.push(CategorySummary {
                category: cat_str,
                missing_count,
                covered_count,
                missing_mods,
            });
        }
    }

    // Remaining categories in name order (a HashMap's order changed from run to run).
    let mut rest: Vec<(String, Vec<ModInfo>)> = cat_map.into_iter().collect();
    rest.sort_by(|a, b| a.0.cmp(&b.0));
    for (cat_str, missing_mods) in rest {
        let missing_count = missing_mods.len();
        let covered_count = mods1.iter().filter(|m| m.category == cat_str && matches_1_to_2.contains_key(&m.filepath)).count();
        category_summaries.push(CategorySummary {
            category: cat_str,
            missing_count,
            covered_count,
            missing_mods,
        });
    }

    // Offline Dependency Check (only real mandatory deps)
    let mut inst2_available_ids: HashSet<String> = HashSet::new();
    for m in mods2 {
        inst2_available_ids.insert(normalize_alphanumeric(&m.mod_id));
        inst2_available_ids.insert(normalize_alphanumeric(&m.canonical_id));
        inst2_available_ids.insert(normalize_alphanumeric(&m.clean_slug));
        for prov in &m.provides {
            inst2_available_ids.insert(normalize_alphanumeric(prov));
        }
        for eq in &m.equivalent_ids {
            inst2_available_ids.insert(normalize_alphanumeric(eq));
        }
    }

    // Map common aliases (e.g. 'fabric' -> fabric-api, 'cloth-config2' -> cloth-config)
    let has_fabric_api = inst2_available_ids.contains("fabricapi") || inst2_available_ids.contains("fabric");
    if has_fabric_api {
        inst2_available_ids.insert("fabric".to_string());
        inst2_available_ids.insert("fabricapi".to_string());
    }
    if inst2_available_ids.contains("clothconfig") || inst2_available_ids.contains("clothconfig2") {
        inst2_available_ids.insert("clothconfig".to_string());
        inst2_available_ids.insert("clothconfig2".to_string());
        inst2_available_ids.insert("clothconfig".to_string());
    }

    let mut dep_map: HashMap<String, Vec<String>> = HashMap::new();
    for m in mods2 {
        for dep in &m.declared_dependencies {
            let norm_dep = normalize_alphanumeric(dep);
            if !norm_dep.is_empty() && !inst2_available_ids.contains(&norm_dep) {
                // If it's a known API or library, flag it
                if let Some(km) = find_known_mod(&norm_dep, &norm_dep) {
                    if km.is_api {
                        dep_map.entry(km.name.to_string()).or_default().push(m.display_name.clone());
                    }
                }
            }
        }
    }

    let mut dependency_issues_inst2 = Vec::new();
    for (dep_name, required_by) in dep_map {
        dependency_issues_inst2.push(DependencyIssue {
            missing_dep_id: dep_name.clone(),
            missing_dep_name: dep_name,
            required_by,
        });
    }

    let common_count = matches_1_to_2.len();
    let missing_count = missing_in_2.len();
    let equivalent_count = equivalents_1_to_2.len();

    ComparisonOutput {
        matches_1_to_2,
        matches_2_to_1,
        missing_in_2,
        equivalents_1_to_2,
        category_summaries,
        dependency_issues_inst2,
        common_count,
        missing_count,
        equivalent_count,
    }
}

pub fn get_user_categories_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let mut p = PathBuf::from(appdata);
        p.push("ModComparer");
        let _ = std::fs::create_dir_all(&p);
        p.push("user_categories.json");
        return p;
    }
    PathBuf::from("user_categories.json")
}

pub fn load_user_categories() -> HashMap<String, String> {
    let path = get_user_categories_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&content) {
                return map;
            }
        }
    }
    HashMap::new()
}

pub const GENERIC_KEYS: &[&str] = &[
    "", "minecraft", "forge", "fabric", "neoforge", "quilt", "liteloader", "rift",
    "mod", "mods", "core", "api", "library", "lib", "client", "server", "java",
];

pub fn is_generic_key(key: &str) -> bool {
    let k = key.trim().to_lowercase();
    k.len() < 3 || GENERIC_KEYS.contains(&k.as_str())
}

pub fn apply_user_categories(mods: &mut [ModInfo]) {
    let user_cats = load_user_categories();
    if user_cats.is_empty() {
        return;
    }
    for m in mods.iter_mut() {
        let id = m.mod_id.to_lowercase();
        let slug = m.clean_slug.to_lowercase();
        let canon = m.canonical_id.to_lowercase();

        let override_cat = (!is_generic_key(&id))
            .then(|| user_cats.get(&id))
            .flatten()
            .or_else(|| {
                (!is_generic_key(&slug))
                    .then(|| user_cats.get(&slug))
                    .flatten()
            })
            .or_else(|| {
                (!is_generic_key(&canon))
                    .then(|| user_cats.get(&canon))
                    .flatten()
            });

        if let Some(cat) = override_cat {
            m.category = canonical_category(cat).to_string();
            if m.category == "APIs y Dependencias" {
                m.is_api = true;
            }
        }
    }
}

