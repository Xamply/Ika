use super::{
    Instance, engine,
    instance_catalog::{self, JavaRuntime},
    java, modpacks,
};
use serde_json::{Value, json};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

pub const LOADERS: [&str; 5] = ["Vanilla", "Fabric", "Forge", "NeoForge", "Quilt"];
pub const FALLBACK_VERSIONS: [&str; 13] = [
    "1.21.11", "1.21.10", "1.21.8", "1.21.5", "1.21.4", "1.21.1", "1.21", "1.20.6", "1.20.4",
    "1.20.1", "1.19.2", "1.16.5", "1.12.2",
];

#[derive(Clone, PartialEq, Eq)]
pub struct Draft {
    pub name: String,
    pub version: String,
    pub loader: String,
    pub loader_version: String,
    pub min_ram: u32,
    pub max_ram: u32,
    pub jvm: String,
    pub java: String,
    pub java_runtime: Option<JavaRuntime>,
    pub cover: Option<PathBuf>,
    pub icon: Option<PathBuf>,
    pub cover_preset: Option<String>,
    pub icon_preset: Option<String>,
}

impl Draft {
    pub fn new(source: Option<&Instance>) -> Self {
        let mut draft = Self {
            name: "Mi instancia".into(),
            version: "1.21.4".into(),
            loader: "Fabric".into(),
            loader_version: String::new(),
            min_ram: 512,
            max_ram: 8192,
            jvm: String::new(),
            java: String::new(),
            java_runtime: None,
            cover: None,
            icon: None,
            cover_preset: None,
            icon_preset: None,
        };
        if let Some(source) = source {
            draft.name.clone_from(&source.name);
            draft.version.clone_from(&source.mc_version);
            draft.loader.clone_from(&source.loader);
            draft.loader_version = source.loader_version.clone().unwrap_or_default();
            let cfg = engine::read_cfg(Path::new(&source.path));
            draft.cover_preset = cfg.get("IkaCoverPreset").filter(|s| !s.is_empty()).cloned();
            draft.icon_preset = cfg.get("IkaIconPreset").filter(|s| !s.is_empty()).cloned();
            draft.min_ram = cfg
                .get("MinMemAlloc")
                .and_then(|s| s.parse().ok())
                .unwrap_or(512);
            draft.max_ram = cfg
                .get("MaxMemAlloc")
                .and_then(|s| s.parse().ok())
                .unwrap_or(8192);
            draft.jvm = cfg
                .get("JvmArgs")
                .map(|s| engine::unescape_ini(s))
                .unwrap_or_default();
            if let (Some(vendor), Some(major)) = (
                cfg.get("IkaJavaVendor").filter(|v| !v.is_empty()),
                cfg.get("IkaJavaMajor").and_then(|v| v.parse::<u32>().ok()),
            ) {
                draft.java_runtime = Some(JavaRuntime {
                    vendor: vendor.clone(),
                    major,
                    name: cfg.get("IkaJavaRuntime").cloned().unwrap_or_default(),
                    url: cfg.get("IkaJavaUrl").cloned().unwrap_or_default(),
                });
            } else if cfg.get("OverrideJavaLocation").is_none_or(|v| v != "false") {
                draft.java = cfg.get("JavaPath").cloned().unwrap_or_default();
            }
        }
        draft
    }
}

pub fn cover_path(instance: &Instance) -> PathBuf {
    Path::new(&instance.path).join("ika-cover.png")
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .build()
}

pub fn versions() -> Vec<String> {
    let response = (|| {
        let res = agent()
            .get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
            .call()
            .ok()?;
        let json: Value = serde_json::from_reader(res.into_reader()).ok()?;
        Some(
            json["versions"]
                .as_array()?
                .iter()
                .filter(|v| v["type"] == "release")
                .filter_map(|v| v["id"].as_str().map(str::to_owned))
                .collect::<Vec<_>>(),
        )
    })();
    response
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| FALLBACK_VERSIONS.map(str::to_owned).to_vec())
}

pub fn java_installations() -> Vec<(u32, String)> {
    java::list_java_installations()
        .into_iter()
        .map(|j| (j.major, j.path))
        .collect()
}

pub fn recommended_java(version: &str) -> u32 {
    let nums = version
        .split('.')
        .filter_map(|p| p.parse::<u32>().ok())
        .collect::<Vec<_>>();
    match nums.as_slice() {
        [1, 0..=16, ..] => 8,
        [1, 17, ..] => 16,
        [1, 18..=19, ..] | [1, 20] | [1, 20, 0..=4, ..] => 17,
        [major, ..] if *major >= 26 => 25,
        _ => 21,
    }
}

fn safe_segment(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let clean = clean.trim_matches([' ', '.']);
    let clean = if clean.is_empty() { "Instancia" } else { clean };
    let prefix = clean
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(prefix.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((prefix.starts_with("COM") || prefix.starts_with("LPT")) && prefix.len() == 4)
    {
        format!("Instancia {clean}")
    } else {
        clean.to_owned()
    }
}

fn atomic_file(path: &Path, content: &[u8]) -> io::Result<()> {
    use io::Write as _;
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    temp.write_all(content)?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

fn cfg_with(old: &str, fields: &[(&str, String)]) -> String {
    let mut out = String::new();
    for line in old.lines() {
        if line
            .split_once('=')
            .is_some_and(|(key, _)| fields.iter().any(|(k, _)| *k == key.trim()))
        {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    for (key, value) in fields {
        out.push_str(&format!("{key}={value}\n"));
    }
    out
}

pub(super) fn version_token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_+".contains(&b))
}

pub fn save(draft: &Draft, original: Option<(&Instance, &Draft)>) -> Result<PathBuf, String> {
    save_inner(draft, original, None)
}

pub fn install_pack(
    draft: &Draft,
    project: &modpacks::Project,
    release: &modpacks::Release,
    progress: impl Fn(String) + Sync,
) -> Result<PathBuf, String> {
    let mut draft = draft.clone();
    checked_name(&draft.name)?;
    let installation = modpacks::install(project, release, &progress)?;
    if draft.icon.is_none()
        && let Some(icon) = modpacks::artwork(&project.icon, 128)
    {
        let _ = icon.save(installation.stage.path().join("ika-icon.png"));
    }
    draft.version.clone_from(&installation.minecraft);
    draft.loader.clone_from(&installation.loader);
    draft
        .loader_version
        .clone_from(&installation.loader_version);
    save_inner(&draft, None, Some(installation))
}

fn checked_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 50 || name.chars().any(char::is_control) {
        return Err("Escribe un nombre de entre 1 y 50 caracteres.".into());
    }
    Ok(name)
}

fn save_inner(
    draft: &Draft,
    original: Option<(&Instance, &Draft)>,
    installation: Option<modpacks::Installation>,
) -> Result<PathBuf, String> {
    let name = checked_name(&draft.name)?;
    let change_version = original.is_none_or(|(_, old)| {
        (
            old.version.as_str(),
            old.loader.as_str(),
            old.loader_version.as_str(),
        ) != (
            draft.version.as_str(),
            draft.loader.as_str(),
            draft.loader_version.as_str(),
        )
    });
    let components = if change_version {
        if !version_token(&draft.version) {
            return Err("Selecciona una versión válida de Minecraft.".into());
        }
        let mut result = vec![
            json!({"uid":"net.minecraft", "version":draft.version, "cachedName":"Minecraft", "cachedVersion":draft.version, "important":true}),
        ];
        if draft.loader != "Vanilla" {
            let (uid, cached, _) =
                engine::loader_uid_and_name(&draft.loader).ok_or("El loader no es compatible.")?;
            let v = if draft.loader_version.is_empty() {
                let versions =
                    instance_catalog::loader_versions(&agent(), &draft.loader, &draft.version)?;
                versions
                    .iter()
                    .find(|v| v.recommended)
                    .or_else(|| versions.first())
                    .map(|v| v.version.clone())
                    .ok_or_else(|| {
                        format!(
                            "No se encontró una versión de {} para {}.",
                            draft.loader, draft.version
                        )
                    })?
            } else {
                draft.loader_version.clone()
            };
            if !version_token(&v) {
                return Err("La versión del loader no es válida.".into());
            }
            result.push(json!({"uid":uid, "version":v, "cachedName":cached, "cachedVersion":v}));
        }
        Some(result)
    } else {
        None
    };

    if draft.min_ram == 0 || draft.max_ram < draft.min_ram || draft.max_ram > 65536 {
        return Err("La RAM máxima debe ser igual o mayor que la mínima.".into());
    }
    if !draft.java.is_empty()
        && original.is_none_or(|(_, old)| old.java != draft.java)
        && !Path::new(&draft.java).is_file()
    {
        return Err("Selecciona un ejecutable Java que exista.".into());
    }

    let new_dir;
    let root = if let Some((source, _)) = original {
        let path = Path::new(&source.path);
        if !path.join("instance.cfg").is_file() || !path.join("mmc-pack.json").is_file() {
            return Err(
                "Solo se pueden guardar cambios en instancias de Ika o compatibles con Prism."
                    .into(),
            );
        }
        path
    } else if let Some(installation) = &installation {
        installation.stage.path()
    } else {
        let root = engine::instances_root();
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        new_dir = tempfile::Builder::new()
            .prefix(".ika-creating-")
            .tempdir_in(&root)
            .map_err(|e| e.to_string())?;
        new_dir.path()
    };

    let cfg = root.join("instance.cfg");
    let pack = root.join("mmc-pack.json");
    let old_cfg = if original.is_some() {
        fs::read_to_string(&cfg).map_err(|e| e.to_string())?
    } else {
        "InstanceType=OneSix\niconKey=default\n".into()
    };
    let mut fields = vec![("name", name.to_owned())];
    for (key, new_art, preset) in [
        ("IkaCoverPreset", &draft.cover, &draft.cover_preset),
        ("IkaIconPreset", &draft.icon, &draft.icon_preset),
    ] {
        if new_art.is_some() {
            fields.push((
                key,
                preset
                    .as_deref()
                    .filter(|s| s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'))
                    .unwrap_or_default()
                    .to_owned(),
            ));
        }
    }
    if change_version {
        fields.extend([
            ("IntendedVersion", draft.version.clone()),
            ("IntendedLoader", draft.loader.clone()),
        ]);
    }
    if original.is_none_or(|(_, old)| (old.min_ram, old.max_ram) != (draft.min_ram, draft.max_ram))
    {
        fields.extend([
            ("OverrideMemory", "true".into()),
            ("MinMemAlloc", draft.min_ram.to_string()),
            ("MaxMemAlloc", draft.max_ram.to_string()),
        ]);
    }
    if original.is_none_or(|(_, old)| old.jvm != draft.jvm) {
        fields.extend([
            ("OverrideJavaArgs", (!draft.jvm.is_empty()).to_string()),
            (
                "JvmArgs",
                serde_json::to_string(&draft.jvm.lines().collect::<Vec<_>>().join(" "))
                    .expect("a string is serializable"),
            ),
        ]);
    }
    if original
        .is_none_or(|(_, old)| old.java != draft.java || old.java_runtime != draft.java_runtime)
    {
        let rt = draft.java_runtime.as_ref();
        if let Some(rt) = rt
            && (!instance_catalog::JAVA_VENDORS
                .iter()
                .any(|vendor| vendor.id == rt.vendor)
                || rt.major == 0
                || rt.name.chars().any(char::is_control)
                || (!rt.url.is_empty() && !instance_catalog::https(&rt.url)))
        {
            return Err("El runtime de Java no es válido.".into());
        }
        fields.extend([
            ("OverrideJavaLocation", (!draft.java.is_empty()).to_string()),
            ("JavaPath", draft.java.replace('\\', "/")),
            (
                "IkaJavaVendor",
                rt.map(|rt| rt.vendor.clone()).unwrap_or_default(),
            ),
            (
                "IkaJavaMajor",
                rt.map(|rt| rt.major.to_string()).unwrap_or_default(),
            ),
            (
                "IkaJavaRuntime",
                rt.map(|rt| rt.name.clone()).unwrap_or_default(),
            ),
            (
                "IkaJavaUrl",
                rt.map(|rt| rt.url.clone()).unwrap_or_default(),
            ),
        ]);
    }
    if let Some(installation) = &installation {
        fields.extend(
            installation
                .cfg
                .iter()
                .map(|(key, value)| (*key, value.lines().collect::<Vec<_>>().join(" "))),
        );
    }

    let art = [
        (&draft.cover, "ika-cover.png", 1280),
        (&draft.icon, "ika-icon.png", 128),
    ]
    .into_iter()
    .filter_map(|(path, file, max)| Some((path.as_ref()?, file, max)))
    .map(|(path, file, max)| -> Result<_, String> {
        let mut reader = image::ImageReader::open(path)
            .map_err(|e| e.to_string())?
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let im = reader
            .decode()
            .map_err(|e| e.to_string())?
            .thumbnail(max, max);
        let mut out = io::Cursor::new(Vec::new());
        im.write_to(&mut out, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        Ok((file, out.into_inner()))
    })
    .collect::<Result<Vec<_>, _>>()?;

    let mut saved_pack = None;
    if let Some(new_components) = components {
        let mut data = if original.is_some() {
            serde_json::from_slice::<Value>(&fs::read(&pack).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?
        } else {
            json!({"formatVersion":1,"components":[]})
        };
        let old = data["components"]
            .as_array_mut()
            .ok_or("El archivo de componentes no es válido.")?;
        // Keep unrelated components and any extra fields when just updating the installed version.
        let mut new = new_components;
        for component in old.drain(..) {
            let uid = component["uid"].as_str().unwrap_or("");
            if let Some(target) = new.iter_mut().find(|c| c["uid"] == uid) {
                if let Some(fields) = component.as_object() {
                    for (k, v) in fields {
                        if target.get(k).is_none()
                            && !matches!(k.as_str(), "cachedRequires" | "cachedConflicts")
                        {
                            target[k] = v.clone();
                        }
                    }
                }
            } else if uid != "net.minecraft" && engine::loader_uid_and_name(uid).is_none() {
                new.push(component);
            }
        }
        *old = new;
        saved_pack = Some(serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?);
    }
    if original.is_none() {
        fs::create_dir_all(root.join(".minecraft/mods")).map_err(|e| e.to_string())?;
    }
    if let Some(bytes) = saved_pack {
        atomic_file(&pack, &bytes).map_err(|e| e.to_string())?;
    }
    for (file, bytes) in art {
        atomic_file(&root.join(file), &bytes).map_err(|e| e.to_string())?;
    }
    atomic_file(&cfg, cfg_with(&old_cfg, &fields).as_bytes()).map_err(|e| e.to_string())?;
    if original.is_some() {
        return Ok(root.to_path_buf());
    }

    let parent = root.parent().unwrap();
    let stem = safe_segment(name);
    for i in 1..1000 {
        let file = if i == 1 {
            stem.clone()
        } else {
            format!("{stem} ({i})")
        };
        let target = parent.join(file);
        if target.exists() {
            continue;
        }
        match fs::rename(root, &target) {
            Ok(()) => return Ok(target),
            Err(_) if target.exists() => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("No hay un nombre de carpeta disponible.".into())
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn export(instance: &Instance, target: &Path) -> Result<(), String> {
    let root = Path::new(&instance.path);
    if !root.is_dir() {
        return Err("Ya no existe la carpeta de la instancia.".into());
    }
    let parent = target.parent().ok_or("Elige una carpeta para el ZIP.")?;
    if parent
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(root.canonicalize().map_err(|e| e.to_string())?)
    {
        return Err("Guarda el ZIP fuera de la carpeta de la instancia.".into());
    }
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    {
        let mut zip = zip::ZipWriter::new(file.as_file_mut());
        let mut add = |base: &Path, relative: &str, prefix: &str| -> io::Result<()> {
            let mut pending = vec![(base.join(relative), format!("{prefix}{relative}"))];
            while let Some((path, name)) = pending.pop() {
                use std::os::windows::fs::MetadataExt;
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(meta) => meta,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(e),
                };
                // Do not follow Windows junctions or symbolic links outside the game.
                if metadata.file_attributes() & 0x400 != 0 {
                    continue;
                }
                if metadata.is_dir() {
                    for entry in fs::read_dir(&path)? {
                        let entry = entry?;
                        pending.push((
                            entry.path(),
                            format!("{name}/{}", entry.file_name().to_string_lossy()),
                        ));
                    }
                } else if metadata.is_file() {
                    let method = if path
                        .extension()
                        .is_some_and(|ext| matches!(ext.to_str(), Some("jar" | "zip" | "png")))
                    {
                        zip::CompressionMethod::Stored
                    } else {
                        zip::CompressionMethod::Deflated
                    };
                    zip.start_file(
                        name,
                        zip::write::SimpleFileOptions::default()
                            .compression_method(method)
                            .large_file(metadata.len() >= u32::MAX as u64),
                    )?;
                    io::copy(&mut fs::File::open(path)?, &mut zip)?;
                }
            }
            Ok(())
        };
        for name in [
            "instance.cfg",
            "mmc-pack.json",
            "instance.json",
            "ika-cover.png",
            "ika-icon.png",
        ] {
            add(root, name, "").map_err(|e| e.to_string())?;
        }
        let game = super::game_folder(instance);
        for name in [
            "mods",
            "config",
            "defaultconfigs",
            "kubejs",
            "scripts",
            "resourcepacks",
            "shaderpacks",
        ] {
            add(&game, name, ".minecraft/").map_err(|e| e.to_string())?;
        }
        zip.finish().map_err(|e| e.to_string())?;
    }
    file.persist(target).map_err(|e| e.error.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_preserves_unrelated_metadata_and_exports_the_playable_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Test");
        fs::create_dir_all(root.join(".minecraft/mods")).unwrap();
        fs::create_dir_all(root.join(".minecraft/saves")).unwrap();
        fs::write(root.join(".minecraft/mods/test.jar"), b"demo mod").unwrap();
        fs::write(root.join(".minecraft/saves/world.txt"), b"private world").unwrap();
        fs::write(root.join("instance.cfg"), "name=Before\nFoo=keep\n").unwrap();
        fs::write(root.join("mmc-pack.json"),
            r#"{"formatVersion":1,"components":[{"uid":"net.minecraft","version":"1.21.4"},{"uid":"org.lwjgl3","version":"3.3.3"}]}"#).unwrap();
        let instance = super::super::catalog::scan(&[temp.path().to_path_buf()], &[])
            .pop()
            .unwrap();
        let old = Draft::new(Some(&instance));
        let mut draft = old.clone();
        draft.name = "After".into();
        draft.version = "1.21.5".into();
        draft.loader = "Fabric".into();
        draft.loader_version = "0.16.14".into();
        draft.java_runtime = Some(JavaRuntime {
            vendor: "adoptium".into(),
            major: 21,
            name: "eclipse_temurin_jre21.0.8+9".into(),
            url: "https://example.test/java.zip".into(),
        });
        draft.jvm = r#"-Dkey="C:\My Folder""#.into();
        save(&draft, Some((&instance, &old))).unwrap();

        let cfg = engine::read_cfg(&root);
        assert_eq!(cfg["name"], "After");
        assert_eq!(cfg["Foo"], "keep");
        assert_eq!(engine::unescape_ini(&cfg["JvmArgs"]), draft.jvm);
        let pack: Value =
            serde_json::from_slice(&fs::read(root.join("mmc-pack.json")).unwrap()).unwrap();
        assert_eq!(pack["components"][0]["version"], "1.21.5");
        assert_eq!(pack["components"][1]["uid"], "net.fabricmc.fabric-loader");
        assert_eq!(pack["components"][1]["version"], "0.16.14");
        assert_eq!(pack["components"][2]["uid"], "org.lwjgl3");
        assert_eq!(cfg["IkaJavaMajor"], "21");
        assert_eq!(cfg["JavaPath"], "");
        let reloaded = Draft::new(Some(&instance));
        assert_eq!(reloaded.java_runtime, draft.java_runtime);
        let mut auto = reloaded.clone();
        auto.java_runtime = None;
        save(&auto, Some((&instance, &reloaded))).unwrap();
        assert!(Draft::new(Some(&instance)).java_runtime.is_none());

        assert!(export(&instance, &root.join("invalid.zip")).is_err());
        let archive_path = temp.path().join("output.zip");
        export(&instance, &archive_path).unwrap();
        let mut archive = zip::ZipArchive::new(fs::File::open(archive_path).unwrap()).unwrap();
        assert!(archive.by_name("instance.cfg").is_ok());
        assert!(archive.by_name("mmc-pack.json").is_ok());
        assert!(archive.by_name(".minecraft/mods/test.jar").is_ok());
        assert!(archive.by_name(".minecraft/saves/world.txt").is_err());
    }
}
