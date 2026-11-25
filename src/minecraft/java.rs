//! Java runtime discovery and installation, ported from Ika WebView2.
use super::engine as mc;

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

#[derive(Clone, Debug)]
pub struct JavaInstallation {
    pub path: String,
    pub major: u32,
}

fn java_major_from_version(v: &str) -> u32 {
    let mut parts = v.split(|c: char| c == '.' || c == '_' || c == '+' || c == '-');
    let first = parts.next().unwrap_or("").parse::<u32>().unwrap_or(0);
    if first == 1 {
        parts
            .next()
            .and_then(|p| p.parse::<u32>().ok())
            .unwrap_or(0)
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
        let folder_name = home
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if let Some(caps) = regex::Regex::new(r#"(\d+\.\d+\.\d+[\.\+]\d+)"#)
            .ok()
            .and_then(|re| re.captures(&folder_name))
        {
            let matched = caps[1].replace('+', ".");
            if matched.starts_with(&version) {
                version = matched;
            }
        }
    }
    if version.is_empty() {
        let java = home.join("bin").join("java.exe");
        if let Ok(out) = create_silent_command(&java.to_string_lossy())
            .arg("-version")
            .output()
        {
            let text = String::from_utf8_lossy(&out.stderr);
            if let Some(caps) = regex::Regex::new(r#"version "([^"]+)""#)
                .ok()
                .and_then(|re| re.captures(&text))
            {
                version = caps[1].to_string();
            }
        }
    }
    if version.is_empty() {
        return None;
    }
    Some(JavaInstallation {
        path: javaw.to_string_lossy().to_string(),
        major: java_major_from_version(&version),
    })
}

const JAVA_DL_USER_AGENT: &str = "IkaLauncher/1.0 (Windows)";

fn curl_get_text(url: &str) -> Result<String, String> {
    let out = create_silent_command("curl.exe")
        .args([
            "-s",
            "-L",
            "-f",
            "--max-time",
            "30",
            "-A",
            JAVA_DL_USER_AGENT,
            url,
        ])
        .output()
        .map_err(|e| format!("Error al ejecutar curl: {}", e))?;
    if !out.status.success() {
        return Err(format!("No se pudo consultar {}", url));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn curl_download_file(url: &str, dest: &std::path::Path) -> Result<(), String> {
    let status = create_silent_command("curl.exe")
        .args([
            "-s",
            "-L",
            "-f",
            "--retry",
            "2",
            "-A",
            JAVA_DL_USER_AGENT,
            "-o",
        ])
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
pub(super) fn java_install_root() -> std::path::PathBuf {
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
            let json: serde_json::Value =
                serde_json::from_str(&curl_get_text(&url)?).map_err(|e| e.to_string())?;
            json.get(0)
                .and_then(|p| p.get("download_url"))
                .and_then(|u| u.as_str())
                .map(str::to_string)
                .ok_or_else(|| format!("Azul Zulu no ofrece Java {} para Windows", major))
        }
        "semeru" => {
            let url = format!(
                "https://api.github.com/repos/ibmruntimes/semeru{}-binaries/releases/latest",
                major
            );
            let json: serde_json::Value =
                serde_json::from_str(&curl_get_text(&url)?).map_err(|e| e.to_string())?;
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
fn download_mojang_manifest_files(
    manifest_url: &str,
    dest: &std::path::Path,
) -> Result<(), String> {
    let manifest: serde_json::Value =
        serde_json::from_str(&curl_get_text(manifest_url)?).map_err(|e| e.to_string())?;
    let files = manifest
        .get("files")
        .and_then(|f| f.as_object())
        .ok_or("Manifiesto de archivos inválido")?;

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
        .args([
            "-s",
            "-f",
            "-L",
            "--retry",
            "2",
            "--create-dirs",
            "--parallel",
            "--parallel-max",
            "16",
            "-A",
            JAVA_DL_USER_AGENT,
            "-K",
        ])
        .arg(&config_path)
        .status()
        .map_err(|e| format!("Error al ejecutar curl: {}", e))?;
    let _ = std::fs::remove_file(&config_path);
    if !status.success() {
        return Err("Falló la descarga del runtime de Mojang".to_string());
    }
    Ok(())
}

fn download_mojang_runtime(
    major: u32,
    dest: &std::path::Path,
    direct_manifest: Option<&str>,
) -> Result<(), String> {
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
        if n == 1 {
            java_major_from_version(name)
        } else {
            n
        }
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
    let key = keys
        .last()
        .ok_or_else(|| format!("Mojang no ofrece Java {}", major))?;
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
pub(super) fn java_runtime_folder(vendor: &str, major: u32, runtime_name: Option<&str>) -> String {
    let clean: String = runtime_name
        .unwrap_or("")
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim_end_matches(['.', ' ']).trim();
    if clean.is_empty() {
        format!("ika-{}-{}", vendor, major)
    } else {
        clean.to_string()
    }
}

pub(super) fn download_java_runtime_inner(
    vendor: &str,
    major: u32,
    direct_url: Option<String>,
    runtime_name: Option<String>,
) -> Result<JavaInstallation, String> {
    let root = java_install_root();
    std::fs::create_dir_all(&root)
        .map_err(|e| format!("No se pudo crear la carpeta de Java: {}", e))?;
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
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| format!("Archivo de Java inválido: {}", e))?;
        archive
            .extract(&staging)
            .map_err(|e| format!("Error al extraer Java: {}", e))?;
        let _ = std::fs::remove_file(&zip_path);
        // Archives contain a top-level folder (e.g. jdk-21.0.5+11-jre)
        let mut homes = Vec::new();
        collect_java_homes(&staging, 2, &mut homes);
        homes
            .into_iter()
            .next()
            .ok_or_else(|| "El archivo descargado no contiene javaw.exe".to_string())
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
    probe_java_home(&final_dir)
        .ok_or_else(|| "Java se descargó pero no se pudo verificar".to_string())
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

pub(super) fn list_java_installations() -> Vec<JavaInstallation> {
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let program_files = env("ProgramFiles");
    let program_files_x86 = env("ProgramFiles(x86)");
    let appdata = env("APPDATA");
    let local_appdata = env("LOCALAPPDATA");
    let user_profile = env("USERPROFILE");

    let mut roots: Vec<(std::path::PathBuf, u32)> = Vec::new();
    for vendor_dir in [
        "Java",
        "Eclipse Adoptium",
        "Eclipse Foundation",
        "Microsoft",
        "Zulu",
        "BellSoft",
        "Amazon Corretto",
        "Semeru",
        "OpenJDK",
    ] {
        roots.push((std::path::Path::new(&program_files).join(vendor_dir), 2));
        roots.push((std::path::Path::new(&program_files_x86).join(vendor_dir), 2));
    }
    roots.push((java_install_root(), 3));
    roots.push((
        std::path::Path::new(&appdata)
            .join("ElyPrismLauncher")
            .join("java"),
        3,
    ));
    roots.push((
        std::path::Path::new(&appdata)
            .join("PrismLauncher")
            .join("java"),
        3,
    ));
    roots.push((
        std::path::Path::new(&appdata)
            .join("FreesmLauncher")
            .join("java"),
        3,
    ));
    roots.push((
        std::path::Path::new(&appdata)
            .join(".battly")
            .join("runtime"),
        4,
    ));
    roots.push((
        std::path::Path::new(&appdata)
            .join(".minecraft")
            .join("runtime"),
        4,
    ));
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
        .filter(|h| {
            seen.insert(
                h.to_string_lossy()
                    .to_lowercase()
                    .trim_end_matches(['\\', '/'])
                    .to_string(),
            )
        })
        .filter_map(|h| probe_java_home(&h))
        .collect();
    result.sort_by(|a, b| b.major.cmp(&a.major).then_with(|| a.path.cmp(&b.path)));
    result
}
