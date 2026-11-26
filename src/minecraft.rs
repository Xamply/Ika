mod catalog;
mod engine;
pub mod instance_catalog;
pub mod instances;
mod java;
pub mod modpacks;
pub mod modpack_catalog;
mod skin_server;

pub use catalog::LauncherInstance as Instance;
pub use engine::{ika_root, instances_root};

#[derive(Clone)]
pub struct Skin {
    pub png: Vec<u8>,
    pub slim: bool,
}

pub fn discover() -> Vec<Instance> {
    catalog::discover()
}

pub fn game_folder(instance: &Instance) -> std::path::PathBuf {
    let root = std::path::Path::new(&instance.path);
    let game = engine::game_dir(root);
    if game.is_dir() {
        game
    } else {
        root.to_path_buf()
    }
}

/// Moves the instance to the launcher's recycle folder (`IkaLauncher/papelera`),
/// out of every list, recoverable. Same layout as WebView2's trash_instance.
pub fn trash(instance: &Instance) -> Result<std::path::PathBuf, String> {
    let root = std::path::Path::new(&instance.path);
    if !root.is_dir() {
        return Err("La carpeta de la instancia no existe".into());
    }
    let trash = ika_root().join("papelera");
    std::fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
    let folder = root.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let target = trash.join(format!("{folder}-{stamp}"));
    if std::fs::rename(root, &target).is_err() {
        // Another drive: copy, then remove the original.
        copy_dir(root, &target).map_err(|e| format!("No se pudo mover la instancia: {e}"))?;
        std::fs::remove_dir_all(root)
            .map_err(|e| format!("Se copió a la papelera, pero no se pudo borrar el original: {e}"))?;
    }
    Ok(target)
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Force-closes a running game by its process id.
pub fn stop(pid: u32) -> Result<(), String> {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess},
    };
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, false, pid).map_err(|e| e.to_string())?;
        let result = TerminateProcess(handle, 1).map_err(|e| e.to_string());
        let _ = CloseHandle(handle);
        result
    }
}

pub fn valid_username(name: &str) -> bool {
    (2..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub fn play(
    instance: &Instance,
    username: &str,
    skin: Option<Skin>,
    notify: &(dyn Fn(&str, usize, usize) + Sync),
    spawned: impl FnOnce(u32),
    started: impl FnOnce(),
) -> Result<(), String> {
    engine::cancel_prewarm();
    catalog::prepare(instance)?;
    engine::run(
        notify,
        std::path::Path::new(&instance.path),
        username,
        skin,
        spawned,
        started,
    )
}

/// Builds the IkaBoost caches of an instance on an invisible desktop, at low
/// priority, so the next Play starts with them ready. Only for instances with
/// IkaBoost=true whose cache is missing or out of date; returns quickly otherwise.
pub fn prewarm(instance: &Instance, username: &str) -> Result<(), String> {
    if !valid_username(username) {
        return Ok(());
    }
    let root = std::path::Path::new(&instance.path);
    if !engine::boost_needs_prewarm(root) {
        return Ok(());
    }
    catalog::prepare(instance)?;
    // Twice: the first run records the classes, the second confirms them.
    engine::prewarm(root, username)?;
    if engine::boost_needs_prewarm(root) {
        engine::prewarm(root, username)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_dir_copies_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("a");
        std::fs::create_dir_all(from.join("mods")).unwrap();
        std::fs::write(from.join("instance.cfg"), "name=A").unwrap();
        std::fs::write(from.join("mods/x.jar"), "jar").unwrap();
        let to = dir.path().join("b");
        copy_dir(&from, &to).unwrap();
        assert_eq!(std::fs::read_to_string(to.join("mods/x.jar")).unwrap(), "jar");
        assert_eq!(std::fs::read_to_string(to.join("instance.cfg")).unwrap(), "name=A");
    }

    #[test]
    fn discovers_existing_and_external_instances_without_writing_to_them() {
        let root = std::env::temp_dir().join(format!("ika-mc-test-{}", std::process::id()));
        let prism = root.join("prism");
        let battly = root.join("battly");
        std::fs::create_dir_all(prism.join(".minecraft/mods")).unwrap();
        std::fs::create_dir_all(&battly).unwrap();
        std::fs::write(prism.join("instance.cfg"), "name=Prism Test\n").unwrap();
        std::fs::write(prism.join("mmc-pack.json"), r#"{"formatVersion":1,"components":[{"uid":"net.minecraft","version":"1.21.1"},{"uid":"net.fabricmc.fabric-loader","version":"0.16.10"}]}"#).unwrap();
        std::fs::write(prism.join(".minecraft/mods/test.jar"), "").unwrap();
        std::fs::write(battly.join("instance.json"), r#"{"name":"Battly Test","version":"1.20.1","loader":"forge","loaderVersion":"47.4.0"}"#).unwrap();

        let instances = catalog::scan(&[root.clone()], &[]);
        assert_eq!(instances.len(), 2);
        assert_eq!(instances[0].mc_version, "1.20.1");
        assert_eq!(instances[0].loader, "Forge");
        assert_eq!(instances[1].loader, "Fabric");
        assert_eq!(instances[1].mod_count, 1);
        assert!(!battly.join("mmc-pack.json").exists());
        catalog::prepare(&instances[0]).unwrap();
        assert_eq!(engine::game_dir(&battly), battly);
        let pack: serde_json::Value =
            serde_json::from_slice(&std::fs::read(battly.join("mmc-pack.json")).unwrap()).unwrap();
        assert_eq!(pack["components"][1]["version"], "47.4.0");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "read-only diagnostic for installed launchers"]
    fn installed_catalog() {
        let start = std::time::Instant::now();
        let catalog = discover();
        let supported = catalog.iter().filter(|i| i.mc_version != "").count();
        println!(
            "{} instances, {} with Minecraft version, {:?}",
            catalog.len(),
            supported,
            start.elapsed()
        );
        assert!(supported > 0);
    }

    #[test]
    #[ignore = "warms the IkaBoost cache of an installed instance (hidden desktop)"]
    fn prewarm_flow() {
        let name = std::env::var("IKA_BENCH").expect("IKA_BENCH");
        let inst = discover().into_iter().find(|i| i.path.ends_with(&name)).expect("instance");
        println!("needs before: {}", engine::boost_needs_prewarm(std::path::Path::new(&inst.path)));
        let t = std::time::Instant::now();
        prewarm(&inst, "Faally").unwrap();
        println!("PREWARM FLOW {:?}, needs after: {}", t.elapsed(), engine::boost_needs_prewarm(std::path::Path::new(&inst.path)));
    }
}
