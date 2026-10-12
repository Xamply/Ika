//! What was read from a file (a mod's metadata and hash, a pack's description...), remembered
//! while the file stays the same: same path, size and modification time. Kept in memory and
//! written to the launcher's data folder, so neither changing instance nor opening the launcher
//! again reads and hashes every .jar and .zip of an instance.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone)]
struct Entry<T> {
    size: u64,
    modified: u64,
    value: T,
}

pub struct DiskCache<T> {
    name: &'static str,
    /// None until the file is read (the first time something is asked for)
    entries: Mutex<Option<HashMap<String, Entry<T>>>>,
    save_pending: AtomicBool,
}

/// The launcher's data folder (the same one as the user's categories).
pub fn data_dir() -> PathBuf {
    let mut dir = std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."));
    dir.push("ModComparer");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Size and modification time (ms since the epoch) of a file; None if it cannot be read.
pub fn file_stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
    Some((meta.len(), modified))
}

impl<T: Serialize + DeserializeOwned + Clone + Send + 'static> DiskCache<T> {
    pub const fn new(name: &'static str) -> Self {
        DiskCache { name, entries: Mutex::new(None), save_pending: AtomicBool::new(false) }
    }

    fn file(&self) -> PathBuf {
        data_dir().join(format!("{}.json", self.name))
    }

    fn with<R>(&self, f: impl FnOnce(&mut HashMap<String, Entry<T>>) -> R) -> R {
        let mut guard = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let map = guard.get_or_insert_with(|| {
            std::fs::read(self.file())
                .ok()
                .and_then(|bytes| serde_json::from_slice::<HashMap<String, Entry<T>>>(&bytes).ok())
                .unwrap_or_default()
        });
        f(map)
    }

    /// What is remembered of the file, if it is still the one it was read from.
    pub fn get(&self, path: &Path, stamp: (u64, u64)) -> Option<T> {
        let key = path.to_string_lossy();
        self.with(|map| map.get(key.as_ref()).filter(|e| (e.size, e.modified) == stamp).map(|e| e.value.clone()))
    }

    pub fn put(&'static self, path: &Path, stamp: (u64, u64), value: T) {
        let key = path.to_string_lossy().to_string();
        self.with(|map| {
            map.insert(key, Entry { size: stamp.0, modified: stamp.1, value });
        });
        self.save_later();
    }

    /// Forgets the files of a folder that are no longer in it (`seen`: the ones that are).
    pub fn forget_missing(&'static self, dir: &Path, seen: &HashSet<String>) {
        let removed = self.with(|map| {
            let before = map.len();
            map.retain(|key, _| seen.contains(key) || Path::new(key).parent() != Some(dir));
            before != map.len()
        });
        if removed {
            self.save_later();
        }
    }

    /// Written a moment later and once, however many files were added meanwhile (a scan adds
    /// hundreds in a row).
    fn save_later(&'static self) {
        if self.save_pending.swap(true, Ordering::SeqCst) {
            return;
        }
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            self.save_pending.store(false, Ordering::SeqCst);
            let json = self.with(|map| serde_json::to_vec(map).ok());
            if let Some(json) = json {
                // written whole or not at all: a file cut short would lose the whole cache
                let file = self.file();
                let tmp = file.with_extension("json.tmp");
                if std::fs::write(&tmp, json).is_ok() {
                    let _ = std::fs::rename(&tmp, &file);
                }
            }
        });
    }
}
