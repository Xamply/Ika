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
