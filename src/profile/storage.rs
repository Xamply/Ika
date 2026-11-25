use crate::minecraft::{Skin, valid_username};
use base64::Engine;
use rusty_leveldb::{Cmp, DB, LdbIterator, Options};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct Account {
    pub username: String,
    #[serde(deserialize_with = "nullable_string")]
    pub display_name: String,
    #[serde(deserialize_with = "nullable_string")]
    pub status_quote: String,
    #[serde(deserialize_with = "nullable_string")]
    pub custom_avatar_url: String,
    #[serde(deserialize_with = "optional_skin_id")]
    pub active_skin_id: Option<String>,
    pub custom_skin_data: Option<String>,
    pub skin_model_type: Option<String>,
}
fn nullable_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}
#[derive(Deserialize)]
#[serde(untagged)]
enum StoredSkinId {
    Text(String),
    Timestamp(u64),
}
impl StoredSkinId {
    fn text(self) -> String {
        match self {
            Self::Text(s) => s,
            Self::Timestamp(n) => n.to_string(),
        }
    }
}
fn skin_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    StoredSkinId::deserialize(d).map(StoredSkinId::text)
}
fn optional_skin_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<StoredSkinId>::deserialize(d)?.map(StoredSkinId::text))
}
impl Account {
    pub fn label(&self) -> &str {
        if self.display_name.trim().is_empty() {
            &self.username
        } else {
            &self.display_name
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct Crop {
    pub pos_x: f32,
    pub pos_y: f32,
    pub zoom: f32,
    pub start_time: f64,
    pub duration: f64,
}
impl Default for Crop {
    fn default() -> Self {
        Self {
            pos_x: 50.,
            pos_y: 50.,
            zoom: 1.,
            start_time: 0.,
            duration: 7.,
        }
    }
}
impl Crop {
    fn sanitized(self) -> Self {
        let d = Self::default();
        let value = |v: f32, fallback, a, b| {
            if v.is_finite() {
                v.clamp(a, b)
            } else {
                fallback
            }
        };
        Self {
            pos_x: value(self.pos_x, d.pos_x, 0., 100.),
            pos_y: value(self.pos_y, d.pos_y, 0., 100.),
            zoom: value(self.zoom, d.zoom, 1., 12.),
            start_time: self.start_time.clamp(0., 86400.),
            duration: self.duration.clamp(0.1, 86400.),
        }
    }
}
pub(super) enum Cover {
    Image(image::RgbaImage),
    Video(PathBuf),
}
#[derive(Deserialize, Serialize)]
struct NativeCover {
    file: String,
    crop: Crop,
}

fn native_cover() -> Option<(Cover, Crop)> {
    let dir = native_dir().join("profile-covers");
    let metadata = fs::read(dir.join("selected.json")).ok()?;
    let saved: NativeCover = serde_json::from_slice(&metadata).ok()?;
    let (id, ext) = saved.file.rsplit_once('.')?;
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let path = dir.join(&saved.file);
    let cover = if matches!(ext, "mp4" | "webm" | "mkv" | "mov" | "m4v" | "ogv" | "gif") {
        path.is_file().then_some(Cover::Video(path))?
    } else if ext == "image" && path.metadata().ok()?.len() <= 32 * 1024 * 1024 {
        Cover::Image(rgba(&fs::read(path).ok()?, 4096)?)
    } else {
        return None;
    };
    Some((cover, saved.crop.sanitized()))
}

pub(super) fn import_cover(path: &Path) -> Result<(Cover, Crop), String> {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    let ext = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let video = matches!(
        ext.as_str(),
        "mp4" | "webm" | "mkv" | "mov" | "m4v" | "ogv" | "gif"
    );
    let limit: u64 = if video {
        512 * 1024 * 1024
    } else {
        32 * 1024 * 1024
    };
    let err = |e: std::io::Error| e.to_string();
    let file = fs::File::open(path).map_err(err)?;
    let metadata = file.metadata().map_err(err)?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err("El archivo no es válido o supera el tamaño admitido para una portada.".into());
    }
    let dir = native_dir().join("profile-covers");
    fs::create_dir_all(&dir).map_err(err)?;
    let mut temp = tempfile::NamedTempFile::new_in(&dir).map_err(err)?;
    let mut file = file.take(limit + 1);
    let mut sha = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buf).map_err(err)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > limit {
            return Err("La portada supera el tamaño admitido.".into());
        }
        sha.update(&buf[..read]);
        temp.write_all(&buf[..read]).map_err(err)?;
    }
    temp.flush().map_err(err)?;
    let name = format!(
        "{:x}.{}",
        sha.finalize(),
        if video { &ext } else { "image" }
    );
    let target = dir.join(&name);
    let cover = if video {
        Cover::Video(target.clone())
    } else {
        Cover::Image(
            rgba(&fs::read(temp.path()).map_err(err)?, 4096)
                .ok_or("Formato de imagen no admitido.")?,
        )
    };
    if !target.exists() {
        temp.persist_noclobber(target)
            .map_err(|e| e.error.to_string())?;
    }
    let crop = Crop::default();
    let payload =
        serde_json::to_vec(&NativeCover { file: name, crop }).map_err(|e| e.to_string())?;
    let mut f = tempfile::NamedTempFile::new_in(&dir).map_err(err)?;
    f.write_all(&payload).map_err(err)?;
    f.persist(dir.join("selected.json"))
        .map_err(|e| e.error.to_string())?;
    Ok((cover, crop))
}
pub(super) fn save_crop(crop: Crop) -> Result<(), String> {
    let bytes = serde_json::to_vec(&crop.sanitized()).map_err(|e| e.to_string())?;
    save_native("profile-cover-crop.json", &bytes)
}
#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct PreferencesGeometry {
    pub origin: Option<[f32; 2]>,
    pub height: Option<f32>,
}

impl PreferencesGeometry {
    fn load(keys: &BTreeMap<String, String>) -> Self {
        let number = |key| {
            keys.get(key)
                .and_then(|s| s.parse::<f32>().ok())
                .filter(|n| n.is_finite())
        };
        fs::read(native_dir().join("preferences-window.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_else(|| Self {
                origin: number("ika_pref_pos_left")
                    .zip(number("ika_pref_pos_top"))
                    .map(|(x, y)| [x, y]),
                height: number("ika_pref_height"),
            })
    }

    pub fn save(self) -> Result<(), String> {
        let data = serde_json::to_vec(&self).map_err(|e| e.to_string())?;
        save_native("preferences-window.json", &data)
    }
}

#[derive(Default)]
pub(super) struct Data {
    pub accounts: Vec<Account>,
    pub active: String,
    pub cover: Option<Cover>,
    pub crop: Crop,
    pub skins: BTreeMap<String, Skin>,
    pub official: BTreeMap<String, Skin>,
    pub wardrobe: BTreeMap<String, Vec<SkinSlot>>,
    pub origin_found: bool,
    pub clock_style: crate::home_widgets::Style,
    pub clock_24h: bool,
    pub favorites: Vec<String>,
    pub friends_autoaccept: bool,
    pub preferences_geometry: PreferencesGeometry,
    pub error: Option<String>,
    pub edits: Edits,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct Edits {
    pub added: Vec<Account>,
    pub removed: Vec<String>,
    pub identity: BTreeMap<String, Identity>,
    pub avatars: BTreeMap<String, String>,
    pub selected_skins: BTreeMap<String, SkinSelection>,
    pub uploaded_skins: BTreeMap<String, Vec<SkinSlot>>,
    pub hidden_skins: BTreeMap<String, Vec<String>>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SkinSlot {
    #[serde(deserialize_with = "skin_id")]
    pub id: String,
    pub name: String,
    pub data_url: String,
    #[serde(default)]
    pub model_type: String,
}
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct SkinSelection {
    pub id: String,
    pub slim: bool,
}
impl SkinSlot {
    pub fn decode(&self, slim: bool) -> Option<Skin> {
        skin(data_url(&self.data_url, 512 * 1024)?, slim)
    }
    pub fn read_file(path: &Path) -> Option<Self> {
        if path.metadata().ok()?.len() > 512 * 1024 {
            return None;
        }
        let png = fs::read(path).ok()?;
        let selected = skin(png, false)?;
        let name = path
            .file_stem()?
            .to_string_lossy()
            .chars()
            .take(48)
            .collect();
        use sha2::{Digest, Sha256};
        let id = format!("native-{:x}", Sha256::digest(&selected.png));
        let data_url = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(selected.png)
        );
        Some(Self {
            id,
            name,
            data_url,
            model_type: "default".into(),
        })
    }
}
impl Data {
    pub fn selection(&self, name: &str) -> SkinSelection {
        if let Some(s) = self.edits.selected_skins.get(&name.to_ascii_lowercase()) {
            return s.clone();
        }
        let account = self.accounts.iter().find(|a| a.username == name);
        SkinSelection {
            id: account
                .and_then(|a| a.active_skin_id.clone())
                .unwrap_or_default(),
            slim: self.skins.get(name).is_some_and(|s| s.slim),
        }
    }
    pub fn apply_skin(&mut self, name: &str, selection: &SkinSelection) {
        let selected = if selection.id.is_empty() {
            self.official.get(name).cloned().map(|mut s| {
                s.slim = selection.slim;
                s
            })
        } else {
            self.wardrobe
                .get(name)
                .and_then(|list| list.iter().find(|s| s.id == selection.id))
                .and_then(|s| s.decode(selection.slim))
        };
        if let Some(s) = selected {
            self.skins.insert(name.to_owned(), s);
        } else {
            self.skins.remove(name);
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Identity {
    pub display_name: String,
    pub status_quote: String,
}
impl Edits {
    fn read() -> Self {
        let path = native_dir().join("profiles.json");
        path.metadata()
            .ok()
            .filter(|m| m.len() < 32 * 1024 * 1024)
            .and_then(|_| fs::read(path).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        if bytes.len() >= 32 * 1024 * 1024 {
            return Err("La colección de skins es demasiado grande.".into());
        }
        save_native("profiles.json", &bytes)
    }
    fn apply(&self, accounts: &mut Vec<Account>) {
        for a in &self.added {
            if valid_username(&a.username)
                && !accounts
                    .iter()
                    .any(|b| b.username.eq_ignore_ascii_case(&a.username))
            {
                accounts.push(a.clone());
            }
        }
        accounts.retain(|a| {
            !self
                .removed
                .iter()
                .any(|n| n.eq_ignore_ascii_case(&a.username))
        });
        for a in accounts {
            if let Some(e) = self.identity.get(&a.username.to_ascii_lowercase()) {
                a.display_name.clone_from(&e.display_name);
                a.status_quote.clone_from(&e.status_quote);
            }
            if let Some(avatar) = self.avatars.get(&a.username.to_ascii_lowercase()) {
                a.custom_avatar_url.clone_from(avatar);
            }
        }
    }
}

pub(super) fn data_url(s: &str, limit: usize) -> Option<Vec<u8>> {
    let (head, s) = s.split_once(',')?;
    if !head.starts_with("data:image/") || !head.ends_with(";base64") || s.len() > limit * 2 {
        return None;
    }
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .ok()
        .filter(|b| b.len() <= limit)
}
pub(super) fn rgba(bytes: &[u8], max: u32) -> Option<image::RgbaImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(max);
    limits.max_image_height = Some(max);
    limits.max_alloc = Some(u64::from(max) * u64::from(max) * 8);
    reader.limits(limits);
    Some(reader.decode().ok()?.into_rgba8())
}
fn skin(bytes: Vec<u8>, slim: bool) -> Option<Skin> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    let (w, h) = rgba(&bytes, 1024)?.dimensions();
    (w >= 64 && w % 64 == 0 && (h == w || h == w / 2)).then_some(Skin { png: bytes, slim })
}

fn local_string(v: &[u8]) -> Option<String> {
    match v.split_first()? {
        (1, tail) => Some(tail.iter().map(|&b| char::from(b)).collect()),
        (0, tail) if tail.len() % 2 == 0 => String::from_utf16(
            &tail
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
        )
        .ok(),
        _ => None,
    }
}
fn decode_local(k: &[u8], v: &[u8]) -> Option<(String, String, String)> {
    let k = k.strip_prefix(b"_")?;
    let i = k.iter().position(|&x| x == 0)?;
    let origin = std::str::from_utf8(&k[..i]).ok()?;
    let key = local_string(&k[i + 1..])?;
    if !matches!(
        key.as_str(),
        "modcomparer_users"
            | "modcomparer_active_user"
            | "ika_removed_accounts"
            | "ika_custom_cover_bg"
            | "ika_custom_cover_pos"
    ) && !key.starts_with("ika_minotar_skin_")
        && !key.starts_with("modcomparer_custom_skins_")
    {
        return None;
    }
    Some((origin.into(), key, local_string(v)?))
}

// Chromium has its own lock and an active WAL. Recover only a private temporary
// copy, and retry if it compacted or an active file changed while being copied.
fn snapshot(
    dir: &Path,
    comparator: Option<Rc<Box<dyn Cmp>>>,
) -> Result<(tempfile::TempDir, DB), String> {
    fn copy(dir: &Path, dest: &Path) -> std::io::Result<bool> {
        let current = fs::read(dir.join("CURRENT"))?;
        let mut total = 0u64;
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name != "CURRENT"
                && !name.starts_with("MANIFEST-")
                && ![".log", ".ldb", ".sst"]
                    .iter()
                    .any(|ext| name.ends_with(ext))
            {
                continue;
            }
            let before = entry.metadata()?;
            if !before.is_file() {
                continue;
            }
            total = total.saturating_add(before.len());
            if total > 256 * 1024 * 1024 {
                return Err(std::io::Error::other(
                    "Almacenamiento WebView2 demasiado grande",
                ));
            }
            fs::copy(entry.path(), dest.join(name.as_ref()))?;
            let after = entry.metadata()?;
            if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
                return Ok(false);
            }
        }
        Ok(fs::read(dir.join("CURRENT"))? == current)
    }
    let mut error = "WebView2 está actualizando sus datos".into();
    for _ in 0..3 {
        let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
        match copy(dir, temp.path()) {
            Ok(true) => {
                let mut options = Options {
                    create_if_missing: false,
                    ..Options::default()
                };
                if let Some(cmp) = &comparator {
                    options.cmp = cmp.clone();
                }
                match DB::open(temp.path(), options) {
                    Ok(db) => return Ok((temp, db)),
                    Err(e) => error = e.to_string(),
                }
            }
            Ok(false) => {}
            Err(e) => error = e.to_string(),
        }
    }
    Err(error)
}

fn prefix(k: &[u8]) -> Option<([u64; 3], &[u8])> {
    let (&b, mut k) = k.split_first()?;
    let mut parts = [0u64; 3];
    for (len, value) in [((b >> 5) & 7) + 1, ((b >> 2) & 7) + 1, (b & 3) + 1]
        .into_iter()
        .zip(&mut parts)
    {
        let (field, rest) = k.split_at_checked(len as usize)?;
        for (i, n) in field.iter().enumerate() {
            *value |= u64::from(*n) << (8 * i);
        }
        k = rest;
    }
    Some((parts, k))
}
fn varint(data: &mut &[u8]) -> Option<u64> {
    let mut n = 0u64;
    for shift in (0..64).step_by(7) {
        let (&b, rest) = data.split_first()?;
        *data = rest;
        if shift == 63 && b > 1 {
            return None;
        }
        n |= u64::from(b & 127) << shift;
        if b < 128 {
            return Some(n);
        }
    }
    None
}
fn utf16be(data: &[u8]) -> Option<String> {
    if data.len() % 2 != 0 {
        return None;
    }
    String::from_utf16(
        &data
            .chunks_exact(2)
            .map(|p| u16::from_be_bytes([p[0], p[1]]))
            .collect::<Vec<_>>(),
    )
    .ok()
}
fn idb_string(data: &mut &[u8]) -> Option<String> {
    let n = usize::try_from(varint(data)?).ok()?.checked_mul(2)?;
    let (s, rest) = data.split_at_checked(n)?;
    *data = rest;
    utf16be(s)
}
struct IdbCmp;
impl Cmp for IdbCmp {
    fn id(&self) -> &'static str {
        "idb_cmp1"
    }
    fn cmp(&self, a: &[u8], b: &[u8]) -> std::cmp::Ordering {
        // This reader walks a snapshot in file order. Compare numeric prefixes
        // and the string keys used by the cover store and database metadata.
        if let (Some((p, a)), Some((q, b))) = (prefix(a), prefix(b)) {
            return p.cmp(&q).then_with(|| {
                if p[2] > 0 {
                    if let (Some(a), Some(b)) = (a.strip_prefix(&[1]), b.strip_prefix(&[1])) {
                        let (mut ar, mut br) = (a, b);
                        if let (Some(a), Some(b)) = (idb_string(&mut ar), idb_string(&mut br)) {
                            return a
                                .encode_utf16()
                                .cmp(b.encode_utf16())
                                .then_with(|| ar.cmp(br));
                        }
                    }
                }
                a.cmp(b)
            });
        }
        a.cmp(b)
    }
    fn find_shortest_sep(&self, a: &[u8], _: &[u8]) -> Vec<u8> {
        a.to_vec()
    }
    fn find_short_succ(&self, a: &[u8]) -> Vec<u8> {
        a.to_vec()
    }
}

fn cover_blob(default: &Path, origin: &str) -> Option<(PathBuf, String)> {
    let (scheme, address) = origin.split_once("://")?;
    let (host, port) = address.split_once(':').unwrap_or((address, "0"));
    if !matches!(scheme, "http" | "https")
        || host.is_empty()
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        || port.parse::<u16>().is_err()
    {
        return None;
    }
    let idb = default.join("IndexedDB");
    let stem = format!("{scheme}_{host}_{port}.indexeddb");
    let (_temp, mut db) = snapshot(
        &idb.join(format!("{stem}.leveldb")),
        Some(Rc::new(Box::new(IdbCmp))),
    )
    .ok()?;
    let mut iter = db.new_iter().ok()?;
    let mut database = None;
    let mut stores = Vec::new();
    let mut blobs = Vec::new();
    while let Some((k, v)) = iter.next() {
        let Some(([db, store, index], mut rest)) = prefix(&k) else {
            continue;
        };
        if [db, store, index] == [0, 0, 0] && rest.first() == Some(&201) {
            rest = &rest[1..];
            if idb_string(&mut rest).is_some()
                && idb_string(&mut rest).as_deref() == Some("IkaCoversDB")
            {
                database = v.get(..8.min(v.len())).filter(|b| !b.is_empty()).map(|b| {
                    b.iter()
                        .enumerate()
                        .map(|(i, &n)| u64::from(n) << (8 * i))
                        .sum::<u64>()
                });
            }
        } else if db > 0 && store == 0 && index == 0 && rest.first() == Some(&50) {
            rest = &rest[1..];
            if let Some(s) = varint(&mut rest)
                && rest == [0]
                && utf16be(&v).as_deref() == Some("media_covers")
            {
                stores.push((db, s));
            }
        } else if index == 3 && rest.first() == Some(&1) {
            rest = &rest[1..];
            if idb_string(&mut rest).as_deref() == Some("profile_cover") && rest.is_empty() {
                blobs.push((db, store, v));
            }
        }
    }
    let db = database?;
    let (_, _, blob) = blobs
        .into_iter()
        .find(|(d, s, _)| *d == db && stores.contains(&(*d, *s)))?;
    let (ty, mut data) = blob.split_first()?;
    if !matches!(ty, 0 | 1) {
        return None;
    }
    let id = varint(&mut data)?;
    let mime = idb_string(&mut data)?;
    let bytes = varint(&mut data)?;
    let path = idb
        .join(format!("{stem}.blob"))
        .join(format!("{db:x}"))
        .join(format!("{:02x}", (id >> 8) & 255))
        .join(format!("{id:x}"));
    (path.metadata().ok()?.len() == bytes).then_some((path, mime))
}

fn own_selection() -> Option<String> {
    fs::read_to_string(native_dir().join("active-profile.txt"))
        .ok()
        .map(|s| s.trim().to_owned())
}
pub(super) fn native_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("IkaNative")
}
pub(super) fn save_selection(name: &str) -> Result<(), String> {
    save_native("active-profile.txt", name.as_bytes())
}
pub(super) fn save_native(file: &str, bytes: &[u8]) -> Result<(), String> {
    let dir = native_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut f = tempfile::NamedTempFile::new_in(&dir).map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut f, bytes).map_err(|e| e.to_string())?;
    f.persist(dir.join(file)).map_err(|e| e.error.to_string())?;
    Ok(())
}

pub(super) fn load() -> Data {
    let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
        return Data::default();
    };
    let mut best: Option<(PathBuf, String, BTreeMap<String, String>)> = None;
    let mut error = None;
    for app in ["com.modcomparer.desktop", "com.modcomparer.app"] {
        let default = local.join(app).join("EBWebView").join("Default");
        let level = default.join("Local Storage").join("leveldb");
        if !level.is_dir() {
            continue;
        }
        let (_temp, mut db) = match snapshot(&level, None) {
            Ok(pair) => pair,
            Err(e) => {
                error = Some(e);
                continue;
            }
        };
        let Ok(mut iter) = db.new_iter() else {
            continue;
        };
        let mut origins = BTreeMap::<String, BTreeMap<String, String>>::new();
        while let Some((k, v)) = iter.next() {
            if let Some((o, k, v)) = decode_local(&k, &v) {
                origins.entry(o).or_default().insert(k, v);
            }
        }
        let score = |origin: &str, keys: &BTreeMap<String, String>| {
            let accounts = keys
                .get("modcomparer_users")
                .and_then(|s| serde_json::from_str::<Vec<Account>>(s).ok());
            let active = keys.get("modcomparer_active_user");
            let has_active = accounts.as_ref().is_some_and(|a| {
                a.iter()
                    .any(|x| active.is_some_and(|n| n.eq_ignore_ascii_case(&x.username)))
            });
            let trusted = matches!(
                origin,
                "http://tauri.localhost"
                    | "https://tauri.localhost"
                    | "tauri://localhost"
                    | "http://127.0.0.1:5173"
                    | "http://localhost:5173"
            );
            (
                trusted,
                has_active,
                accounts.is_some_and(|a| !a.is_empty()),
                keys.contains_key("ika_custom_cover_bg"),
            )
        };
        if let Some((origin, keys)) = origins
            .into_iter()
            .filter(|(o, k)| score(o, k).0)
            .max_by_key(|(o, k)| score(o, k))
            && best
                .as_ref()
                .is_none_or(|(_, o, k)| score(&origin, &keys) > score(o, k))
        {
            best = Some((default, origin, keys));
        }
    }
    let origin_found = best.is_some();
    let (default, origin, keys) = best.unwrap_or_default();
    let mut accounts = keys
        .get("modcomparer_users")
        .and_then(|s| serde_json::from_str::<Vec<Account>>(s).ok())
        .unwrap_or_default();
    let removed = keys
        .get("ika_removed_accounts")
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default();
    accounts.retain(|a| {
        valid_username(&a.username) && !removed.iter().any(|r| r.eq_ignore_ascii_case(&a.username))
    });
    let edits = Edits::read();
    edits.apply(&mut accounts);
    let native_selection = own_selection();
    let signed_out = native_selection.as_deref() == Some("");
    let active = native_selection
        .or_else(|| keys.get("modcomparer_active_user").cloned())
        .and_then(|n| {
            accounts
                .iter()
                .find(|a| a.username.eq_ignore_ascii_case(&n))
                .map(|a| a.username.clone())
        })
        .or_else(|| {
            (!signed_out)
                .then(|| accounts.first().map(|a| a.username.clone()))
                .flatten()
        })
        .unwrap_or_default();
    let mut skins = BTreeMap::new();
    let mut official = BTreeMap::new();
    let mut wardrobe = BTreeMap::new();
    for a in &accounts {
        let name = a.username.to_ascii_lowercase();
        let cache = keys
            .get(&format!("ika_minotar_skin_{name}"))
            .map(String::as_str);
        let mut slots = keys
            .get(&format!("modcomparer_custom_skins_{name}"))
            .and_then(|s| serde_json::from_str::<Vec<SkinSlot>>(s).ok())
            .unwrap_or_default();
        if let (Some(id), Some(data_url)) = (&a.active_skin_id, &a.custom_skin_data)
            && !id.is_empty()
            && !slots.iter().any(|s| s.id == *id)
        {
            slots.push(SkinSlot {
                id: id.clone(),
                name: "Personalizada".into(),
                data_url: data_url.clone(),
                model_type: a.skin_model_type.clone().unwrap_or_default(),
            });
        }
        if let Some(extra) = edits.uploaded_skins.get(&name) {
            slots.extend(extra.iter().cloned());
        }
        if let Some(removed) = edits.hidden_skins.get(&name) {
            slots.retain(|slot| !removed.iter().any(|id| *id == slot.id));
        }
        let default = account_skin(a, cache);
        if let Some(s) = &default {
            official.insert(a.username.clone(), s.clone());
        }
        let saved = edits.selected_skins.get(&name);
        let wanted = saved
            .map(|s| s.id.as_str())
            .or(a.active_skin_id.as_deref())
            .unwrap_or("");
        let selected = slots
            .iter()
            .find(|s| s.id == wanted)
            .and_then(|s| {
                s.decode(saved.map_or(a.skin_model_type.as_deref() == Some("slim"), |s| s.slim))
            })
            .or_else(|| {
                default.map(|mut s| {
                    if let Some(saved) = saved {
                        s.slim = saved.slim;
                    }
                    s
                })
            });
        if let Some(s) = selected {
            skins.insert(a.username.clone(), s);
        }
        wardrobe.insert(a.username.clone(), slots);
    }
    let original_cover = || {
        keys.get("ika_custom_cover_bg").and_then(|s| {
            if s == "idb:profile_cover" {
                let (path, mime) = cover_blob(&default, &origin)?;
                if mime.starts_with("video/") || mime == "image/gif" {
                    Some(Cover::Video(path))
                } else if mime.starts_with("image/")
                    && path.metadata().ok()?.len() <= 32 * 1024 * 1024
                {
                    Some(Cover::Image(rgba(&fs::read(path).ok()?, 4096)?))
                } else {
                    None
                }
            } else {
                Some(Cover::Image(rgba(&data_url(s, 32 * 1024 * 1024)?, 4096)?))
            }
        })
    };
    let original_crop = || {
        keys.get("ika_custom_cover_pos")
            .and_then(|s| serde_json::from_str::<Crop>(s).ok())
            .unwrap_or_default()
            .sanitized()
    };
    let (cover, crop) = native_cover()
        .map(|(cover, crop)| (Some(cover), crop))
        .unwrap_or_else(|| (original_cover(), original_crop()));
    let crop = fs::read(native_dir().join("profile-cover-crop.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Crop>(&b).ok())
        .map(Crop::sanitized)
        .unwrap_or(crop);
    Data {
        accounts,
        active,
        cover,
        crop,
        skins,
        official,
        wardrobe,
        origin_found,
        clock_style: keys
            .get("ika_clock_style")
            .map(|s| crate::home_widgets::Style::from_webview(s))
            .unwrap_or_default(),
        clock_24h: keys.get("ika_clock_24h").is_some_and(|s| s == "true"),
        favorites: keys
            .get("modcomparer_favorites")
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default(),
        preferences_geometry: PreferencesGeometry::load(&keys),
        friends_autoaccept: keys.get("ika_friends_autoaccept").is_some_and(|s| s != "0"),
        error: if origin_found { None } else { error },
        edits,
    }
}
fn account_skin(a: &Account, cache: Option<&str>) -> Option<Skin> {
    let slim = a.skin_model_type.as_deref().map(|m| m == "slim");
    let decode = |s: &str, model| skin(data_url(s, 4 * 1024 * 1024)?, slim.unwrap_or(model));
    if let Some(app) = std::env::var_os("APPDATA").map(PathBuf::from) {
        for name in ["ElyPrismLauncher", "PrismLauncher", "FreesmLauncher"] {
            let path = app.join(name).join("accounts.json");
            if !path.metadata().is_ok_and(|m| m.len() < 16 * 1024 * 1024) {
                continue;
            }
            let Ok(bytes) = fs::read(path) else { continue };
            let Ok(data) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            let Some(accounts) = data["accounts"].as_array() else {
                continue;
            };
            for entry in accounts {
                if !entry
                    .pointer("/profile/name")
                    .and_then(|v| v.as_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(&a.username))
                {
                    continue;
                }
                let model = entry
                    .pointer("/profile/skin/variant")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case("slim"));
                if let Some(raw) = entry.pointer("/profile/skin/data").and_then(|v| v.as_str()) {
                    if let Some(s) = decode(raw, model).or_else(|| {
                        skin(
                            base64::engine::general_purpose::STANDARD.decode(raw).ok()?,
                            slim.unwrap_or(model),
                        )
                    }) {
                        return Some(s);
                    }
                }
            }
        }
    }
    cache.and_then(|s| decode(s, false))
}
pub(super) fn public_skin(a: &Account) -> Option<Skin> {
    use std::io::Read;
    if !valid_username(&a.username) {
        return None;
    }
    let response = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_millis(1200))
        .build()
        .get(&format!("https://minotar.net/skin/{}", a.username))
        .call()
        .ok()?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 4 * 1024 * 1024 {
        return None;
    }
    skin(bytes, a.skin_model_type.as_deref() == Some("slim"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_storage_encodings_and_scoped_keys() {
        assert_eq!(local_string(&[1, 0xe9]).as_deref(), Some("é"));
        assert_eq!(
            local_string(&[0, 0x3d, 0xd8, 0x00, 0xde]).as_deref(),
            Some("😀")
        );
        assert!(decode_local(b"_http://tauri.localhost\0\x01other", b"\x01secret").is_none());
    }
    #[test]
    fn webview_skin_timestamp_matches_the_selected_skin() {
        let account: Account = serde_json::from_str(
            r#"{"username":"Player","activeSkinId":1741234567890,"customAvatarUrl":null}"#,
        )
        .unwrap();
        let item: SkinSlot =
            serde_json::from_str(r#"{"id":1741234567890,"name":"Personalizada","dataUrl":""}"#)
                .unwrap();
        assert_eq!(account.active_skin_id.as_deref(), Some(item.id.as_str()));
        assert!(account.custom_avatar_url.is_empty());
    }
    #[test]
    fn resolves_live_indexeddb_cover_without_assuming_database_or_blob_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let scope = "https_tauri.localhost_0.indexeddb";
        let idb = tmp.path().join("IndexedDB");
        let leveldb = idb.join(format!("{scope}.leveldb"));
        fs::create_dir_all(&idb).unwrap();
        fn str16(s: &str) -> Vec<u8> {
            s.encode_utf16().flat_map(u16::to_be_bytes).collect()
        }
        fn key_string(s: &str) -> Vec<u8> {
            [vec![s.len() as u8], str16(s)].concat()
        }
        let options = Options {
            cmp: Rc::new(Box::new(IdbCmp)),
            ..Options::default()
        };
        let mut db = DB::open(leveldb, options).unwrap();
        db.put(
            &[
                vec![0, 0, 0, 0, 201],
                key_string("https://tauri.localhost"),
                key_string("IkaCoversDB"),
            ]
            .concat(),
            &[7],
        )
        .unwrap();
        db.put(&[0, 7, 0, 0, 50, 12, 0], &str16("media_covers"))
            .unwrap();
        db.put(
            &[vec![0, 7, 12, 3, 1], key_string("profile_cover")].concat(),
            &[vec![0, 0x81, 0x02], key_string("video/mp4"), vec![4]].concat(),
        )
        .unwrap();
        db.close().unwrap();
        let path = idb.join(format!("{scope}.blob/7/01/101"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, [0x10, 0x20, 0x30, 0x40]).unwrap();
        let (read, mime) = cover_blob(tmp.path(), "https://tauri.localhost").unwrap();
        assert_eq!(read, path);
        assert_eq!(mime, "video/mp4");
    }
    #[test]
    #[ignore = "read-only check of the current Windows profile"]
    fn existing_webview() {
        let d = load();
        assert!(d.origin_found, "WebView2: {:?}", d.error);
        assert!(!d.active.is_empty());
        println!(
            "{} cuentas; portada: {}; skins locales: {}",
            d.accounts.len(),
            d.cover.is_some(),
            d.skins.len()
        );
        assert!(d.cover.is_some());
        assert!(d.skins.contains_key(&d.active));
        let state = super::super::social::sync(&d.active).unwrap();
        println!("{} amigos", state.friends.len());
    }
}
