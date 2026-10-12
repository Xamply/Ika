//! Sending an instance to a friend, from one PC to the other.
//!
//! Two launchers find each other through the mailbox of the skins service (skins-service/):
//! small signed messages between two player names, an invitation and the WebRTC offer/answer.
//! The instance itself never goes through a server: it travels over the direct connection the
//! frontend opens with those messages. This module is the rest: the signed mailbox calls, the
//! package that is sent (the instance without anything tied to this PC or this player) and
//! turning a received package into a new instance.

use base64::Engine;
use ed25519_dalek::Signer;
use sha2::Digest;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// Reusable HTTP agent keeping TCP and TLS connections alive across chunks and polling calls.
/// Eliminates TLS handshake latency (300-600ms per chunk) on international transfers.
static HTTP_AGENT: OnceLock<ureq::Agent> = OnceLock::new();

fn http_agent() -> &'static ureq::Agent {
    HTTP_AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(15))
            .timeout_read(std::time::Duration::from_secs(45))
            .timeout_write(std::time::Duration::from_secs(45))
            .build()
    })
}

/// What travels from the game folder: what makes the instance play the same. No worlds,
/// screenshots, logs or caches.
const GAME_DIRS: [&str; 6] = ["mods", "config", "shaderpacks", "resourcepacks", "defaultconfigs", "kubejs"];
const GAME_FILES: [&str; 4] = ["options.txt", "optionsshaders.txt", "optionsof.txt", "servers.dat"];

/// instance.cfg keys that only make sense on the PC (or for the player) they come from: where
/// Java is installed, play time, the account. Java is chosen again by the receiving launcher.
const CFG_DROP: [&str; 9] = [
    "JavaPath",
    "OverrideJavaLocation",
    "OverrideJava",
    "lastLaunchTime",
    "lastTimePlayed",
    "totalTimePlayed",
    "InstanceAccountId",
    "UseAccountForInstance",
    "linkedInstances",
];

fn share_dir() -> PathBuf {
    crate::mc::ika_root().join("share")
}

/// Share ids come from the frontend and end up in file names.
fn check_id(id: &str) -> Result<(), String> {
    let ok = (8..=40).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    ok.then_some(()).ok_or_else(|| "Identificador de envío inválido".to_string())
}

fn outgoing_path(id: &str) -> PathBuf {
    share_dir().join(format!("out-{}.zip", id))
}

fn incoming_path(id: &str) -> PathBuf {
    share_dir().join(format!("in-{}.part", id))
}

fn game_dir(root: &Path) -> PathBuf {
    if root.join(".minecraft").is_dir() {
        root.join(".minecraft")
    } else if root.join("minecraft").is_dir() {
        root.join("minecraft")
    } else if root.join("instance.json").is_file() || root.join("mods").is_dir() {
        root.to_path_buf()
    } else {
        root.join("minecraft")
    }
}

// ---------------------------------------------------------------------------------------------
// Mailbox (signed with this PC's device key)
// ---------------------------------------------------------------------------------------------

/// The device key, read once: getting it runs `reg` and DPAPI, and the mailbox is polled.
static DEVICE_KEY: Mutex<Option<ed25519_dalek::SigningKey>> = Mutex::new(None);

fn device_key() -> Result<ed25519_dalek::SigningKey, String> {
    let mut cached = DEVICE_KEY.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(key) = cached.as_ref() {
        return Ok(key.clone());
    }
    let key = crate::device_key::device_key()?;
    *cached = Some(key.clone());
    Ok(key)
}

fn valid_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    let ok = (2..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    ok.then(|| name.to_lowercase())
        .ok_or_else(|| format!("«{}» no es un nombre de jugador válido (2 a 16 letras, números o _)", name))
}

fn valid_code(raw: &str) -> Result<String, String> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 6 {
        Ok(digits)
    } else {
        Err("Introduce un código válido de 6 dígitos (por ejemplo: 582-194).".to_string())
    }
}

/// One signed call to the mailbox/room service. `path` is the path with its query, exactly what is signed.
fn mailbox(method: &str, path: &str, body: &[u8]) -> Result<serde_json::Value, String> {
    let service = crate::device_key::skins_service_url().ok_or("El servicio de Ika no está configurado")?;
    let key = device_key()?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
        .to_string();
    let digest: String = sha2::Sha256::digest(body).iter().map(|b| format!("{:02x}", b)).collect();
    let signature = key.sign(format!("ika-share-v1\n{}\n{}\n{}\n{}", method, path, ts, digest).as_bytes());
    let request = http_agent()
        .request(method, &format!("{}{}", service, path))
        .set("x-ika-pubkey", &B64.encode(key.verifying_key().to_bytes()))
        .set("x-ika-timestamp", &ts)
        .set("x-ika-signature", &B64.encode(signature.to_bytes()));
    let response = if method == "POST" {
        request.set("content-type", "application/json").send_bytes(body)
    } else if method == "PUT" {
        request.set("content-type", "application/octet-stream").send_bytes(body)
    } else {
        request.call()
    };
    match response {
        Ok(r) => Ok(r.into_string().ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()),
        Err(ureq::Error::Status(code, r)) => {
            let detail = r
                .into_string()
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
                .unwrap_or_default();
            Err(format!("{}|{}", code, detail))
        }
        Err(e) => Err(format!("0|{}", e)),
    }
}

/// Raw binary fetch for relay chunks.
fn mailbox_raw(method: &str, path: &str, body: &[u8]) -> Result<Vec<u8>, String> {
    let service = crate::device_key::skins_service_url().ok_or("El servicio de Ika no está configurado")?;
    let key = device_key()?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
        .to_string();
    let digest: String = sha2::Sha256::digest(body).iter().map(|b| format!("{:02x}", b)).collect();
    let signature = key.sign(format!("ika-share-v1\n{}\n{}\n{}\n{}", method, path, ts, digest).as_bytes());
    let request = http_agent()
        .request(method, &format!("{}{}", service, path))
        .set("x-ika-pubkey", &B64.encode(key.verifying_key().to_bytes()))
        .set("x-ika-timestamp", &ts)
        .set("x-ika-signature", &B64.encode(signature.to_bytes()));
    match request.call() {
        Ok(r) => {
            let mut bytes = Vec::new();
            r.into_reader().read_to_end(&mut bytes).map_err(|e| format!("0|{}", e))?;
            Ok(bytes)
        }
        Err(ureq::Error::Status(code, r)) => {
            let detail = r
                .into_string()
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
                .unwrap_or_default();
            Err(format!("{}|{}", code, detail))
        }
        Err(e) => Err(format!("0|{}", e)),
    }
}

/// Mailbox errors as "code|detail" turned into what the user reads.
fn explain(err: String, me: &str, other: Option<&str>) -> String {
    let (code, detail) = err.split_once('|').unwrap_or(("", err.as_str()));
    match (code, detail) {
        ("404", "unknown recipient") => format!(
            "No existe ningún jugador «{}» en Ika. Tu amigo tiene que abrir su launcher con ese nombre al menos una vez.",
            other.unwrap_or("")
        ),
        ("403", _) => format!("El nombre «{}» ya pertenece a otro PC: usa otro nombre de jugador para enviar y recibir instancias.", me),
        ("0", _) => "No se pudo conectar con el servicio de Ika. Revisa tu conexión a internet.".to_string(),
        _ => format!("El servicio de Ika respondió {}: {}", code, detail),
    }
}

fn explain_room(err: String, room_code: Option<&str>) -> String {
    let (status, detail) = err.split_once('|').unwrap_or(("", err.as_str()));
    match (status, detail) {
        ("404", "room not found") => {
            if let Some(c) = room_code {
                let fmt = if c.len() == 6 { format!("{}-{}", &c[..3], &c[3..]) } else { c.to_string() };
                format!("El código «{}» no existe o ya expiró. Pídele a tu amigo que genere uno nuevo.", fmt)
            } else {
                "El código de transferencia ya expiró o fue cerrado.".to_string()
            }
        }
        ("403", "room already in use") => "Este código ya está siendo utilizado por otro jugador.".to_string(),
        ("410", _) => "El envío fue cancelado por el otro jugador.".to_string(),
        ("0", _) => "No se pudo conectar con el servicio de Ika. Revisa tu conexión a internet.".to_string(),
        _ => format!("El servicio de Ika respondió {}: {}", status, detail),
    }
}

/// Creates a 6-digit room for an outgoing instance transfer.
#[tauri::command(async)]
pub fn share_room_create(id: String, sender: String, meta: serde_json::Value, offer: Option<String>) -> Result<serde_json::Value, String> {
    check_id(&id)?;
    let payload = serde_json::json!({
        "id": id,
        "sender": sender.trim(),
        "meta": meta,
        "offer": offer,
    });
    let body = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
    mailbox("POST", "/share/room", &body).map_err(|e| explain_room(e, None))
}

/// Reads the current state of a 6-digit room.
#[tauri::command(async)]
pub fn share_room_get(code: String) -> Result<serde_json::Value, String> {
    let code = valid_code(&code)?;
    mailbox("GET", &format!("/share/room/{}", code), &[]).map_err(|e| explain_room(e, Some(&code)))
}

/// Updates a 6-digit room (e.g. receiver submits WebRTC answer, or switches to relay mode).
#[tauri::command(async)]
pub fn share_room_update(code: String, patch: serde_json::Value) -> Result<serde_json::Value, String> {
    let code = valid_code(&code)?;
    let body = serde_json::to_vec(&patch).map_err(|e| e.to_string())?;
    mailbox("POST", &format!("/share/room/{}", code), &body).map_err(|e| explain_room(e, Some(&code)))
}

/// Closes and deletes a 6-digit room.
#[tauri::command(async)]
pub fn share_room_close(code: String) -> Result<(), String> {
    if let Ok(code) = valid_code(&code) {
        let _ = mailbox("DELETE", &format!("/share/room/{}", code), &[]);
    }
    Ok(())
}

/// Uploads a chunk of the outgoing package to the room's relay fallback.
#[tauri::command(async)]
pub fn share_room_put_chunk(code: String, id: String, index: u32, offset: u64, len: u32) -> Result<serde_json::Value, String> {
    let code = valid_code(&code)?;
    check_id(&id)?;
    let mut file = std::fs::File::open(outgoing_path(&id)).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    let mut buf = Vec::with_capacity(len as usize);
    file.take(len as u64).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    let res = mailbox("PUT", &format!("/share/room/{}/chunk/{}", code, index), &buf)
        .map_err(|e| explain_room(e, Some(&code)))?;
    let wait = res.get("wait").and_then(|v| v.as_bool()).unwrap_or(false);
    Ok(serde_json::json!({
        "ok": !wait,
        "wait": wait,
        "sent": if wait { 0 } else { buf.len() },
    }))
}

/// Downloads a chunk from the room's relay fallback and appends it directly to the incoming file.
#[tauri::command(async)]
pub fn share_room_get_chunk(code: String, id: String, index: u32) -> Result<serde_json::Value, String> {
    let code = valid_code(&code)?;
    check_id(&id)?;
    match mailbox_raw("GET", &format!("/share/room/{}/chunk/{}", code, index), &[]) {
        Ok(bytes) => {
            let mut map = INCOMING.lock().unwrap_or_else(|e| e.into_inner());
            let incoming = map.as_mut().and_then(|m| m.get_mut(&id)).ok_or("No hay ninguna recepción en curso")?;
            incoming.file.write_all(&bytes).map_err(|e| format!("No se pudo guardar lo recibido: {}", e))?;
            incoming.hasher.update(&bytes);
            Ok(serde_json::json!({ "ready": true, "bytes": bytes.len() }))
        }
        Err(err) if err.starts_with("404|") => {
            Ok(serde_json::json!({ "ready": false, "bytes": 0 }))
        }
        Err(err) => Err(explain_room(err, Some(&code))),
    }
}

/// Messages waiting for this player (also makes the name this PC's if nobody uses it yet).
#[tauri::command(async)]
pub fn share_box_get(username: String) -> Result<serde_json::Value, String> {
    let me = valid_name(&username)?;
    let reply = mailbox("GET", &format!("/share/box/{}", me), &[]).map_err(|e| explain(e, &me, None))?;
    Ok(reply.get("messages").cloned().unwrap_or_else(|| serde_json::json!([])))
}

/// Leaves a message `{id, type, data}` for another player.
#[tauri::command(async)]
pub fn share_box_send(username: String, to: String, message: serde_json::Value) -> Result<(), String> {
    let me = valid_name(&username)?;
    let to = valid_name(&to)?;
    let body = serde_json::to_vec(&message).map_err(|e| e.to_string())?;
    mailbox("POST", &format!("/share/box/{}?from={}", to, me), &body).map_err(|e| explain(e, &me, Some(&to)))?;
    Ok(())
}

/// Removes messages this player has dealt with, by the `key` the service gave each one.
#[tauri::command(async)]
pub fn share_box_ack(username: String, keys: Vec<String>) -> Result<(), String> {
    let me = valid_name(&username)?;
    for key in &keys {
        check_id(key)?;
    }
    if keys.is_empty() {
        return Ok(());
    }
    mailbox("DELETE", &format!("/share/box/{}?keys={}", me, keys.join(",")), &[]).map_err(|e| explain(e, &me, None))?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Sending: the package
// ---------------------------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct Package {
    size: u64,
    sha256: String,
    files: u32,
    mods: u32,
}

fn add_file(zip: &mut zip::ZipWriter<std::fs::File>, path: &Path, name: &str) -> Result<(), String> {
    // Jars, zips, pngs and audio are already compressed so we store them without CPU waste.
    // Configs, scripts, json, toml, txt, yml, csv, etc. compress up to 80-90% with fast Deflate.
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    let already_compressed = matches!(
        ext.as_str(),
        "jar" | "zip" | "gz" | "xz" | "png" | "jpg" | "jpeg" | "webp" | "ogg" | "mp3" | "wav"
    );
    let method = if already_compressed {
        zip::CompressionMethod::Stored
    } else {
        zip::CompressionMethod::Deflated
    };

    let options = zip::write::SimpleFileOptions::default()
        .compression_method(method)
        .large_file(std::fs::metadata(path).map(|m| m.len() > u32::MAX as u64).unwrap_or(false));
    let mut file = std::fs::File::open(path).map_err(|e| format!("No se pudo leer {}: {}", path.display(), e))?;
    zip.start_file(name, options).map_err(|e| e.to_string())?;
    std::io::copy(&mut file, zip).map_err(|e| format!("No se pudo empaquetar {}: {}", path.display(), e))?;
    Ok(())
}

/// Adds a folder; returns how many files it had.
fn add_dir(zip: &mut zip::ZipWriter<std::fs::File>, dir: &Path, prefix: &str) -> Result<u32, String> {
    let mut count = 0;
    for entry in walkdir::WalkDir::new(dir).follow_links(false).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(dir) else { continue };
        let rel = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        add_file(zip, entry.path(), &format!("{}/{}", prefix, rel))?;
        count += 1;
    }
    Ok(count)
}

/// Builds the package of an instance in the share folder as out-{id}.zip:
/// instance.cfg (without what belongs to this PC), mmc-pack.json and patches/ say which
/// Minecraft and loader it is, the receiving launcher downloads those and Java by itself;
/// minecraft/ carries mods, config, shaders, resource packs and options.
#[tauri::command(async)]
pub fn share_pack(instance_path: String, id: String) -> Result<Package, String> {
    check_id(&id)?;
    let root = crate::instance_root_dir(&instance_path)?;
    let game = game_dir(&root);
    std::fs::create_dir_all(share_dir()).map_err(|e| e.to_string())?;
    let out = outgoing_path(&id);
    let result = (|| -> Result<(u32, u32), String> {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&out).map_err(|e| e.to_string())?);
        let text = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let cfg: Vec<String> = std::fs::read_to_string(root.join("instance.cfg"))
            .unwrap_or_default()
            .lines()
            .filter(|line| !CFG_DROP.iter().any(|key| line.split_once('=').is_some_and(|(k, _)| k.trim() == *key)))
            .map(str::to_string)
            .collect();
        zip.start_file("instance.cfg", text).map_err(|e| e.to_string())?;
        zip.write_all(cfg.join("\n").as_bytes()).map_err(|e| e.to_string())?;

        let mut files = 1;
        add_file(&mut zip, &root.join("mmc-pack.json"), "mmc-pack.json")?;
        files += 1;
        if root.join("patches").is_dir() {
            files += add_dir(&mut zip, &root.join("patches"), "patches")?;
        }
        for dir in GAME_DIRS {
            if game.join(dir).is_dir() {
                files += add_dir(&mut zip, &game.join(dir), &format!("minecraft/{}", dir))?;
            }
        }
        // the mods themselves, enabled or not (the folder also holds metadata files)
        let mods = std::fs::read_dir(game.join("mods"))
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| {
                        let name = e.file_name().to_string_lossy().to_lowercase();
                        name.ends_with(".jar") || name.ends_with(".jar.disabled")
                    })
                    .count() as u32
            })
            .unwrap_or(0);
        for name in GAME_FILES {
            if game.join(name).is_file() {
                add_file(&mut zip, &game.join(name), &format!("minecraft/{}", name))?;
                files += 1;
            }
        }
        zip.finish().map_err(|e| e.to_string())?;
        Ok((files, mods))
    })();
    let (files, mods) = match result {
        Ok(counts) => counts,
        Err(e) => {
            let _ = std::fs::remove_file(&out);
            return Err(e);
        }
    };

    let mut hasher = sha2::Sha256::new();
    let mut file = std::fs::File::open(&out).map_err(|e| e.to_string())?;
    std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
    Ok(Package {
        size: std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0),
        sha256: hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect(),
        files,
        mods,
    })
}

/// A piece of the package being sent, as raw bytes for the data channel.
#[tauri::command(async)]
pub fn share_read_chunk(id: String, offset: u64, len: u32) -> Result<tauri::ipc::Response, String> {
    check_id(&id)?;
    let mut file = std::fs::File::open(outgoing_path(&id)).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    let mut buf = Vec::with_capacity(len as usize);
    file.take(len as u64).read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(buf))
}

/// The package is no longer needed (sent, declined or cancelled).
#[tauri::command(async)]
pub fn share_discard(id: String) {
    if check_id(&id).is_ok() {
        let _ = std::fs::remove_file(outgoing_path(&id));
    }
}

/// Leftovers of a previous session (the launcher was closed mid-transfer).
#[tauri::command(async)]
pub fn share_cleanup() {
    let _ = std::fs::remove_dir_all(share_dir());
}

// ---------------------------------------------------------------------------------------------
// Receiving
// ---------------------------------------------------------------------------------------------

struct Incoming {
    file: std::fs::File,
    hasher: sha2::Sha256,
}

/// Packages being received, by share id.
static INCOMING: Mutex<Option<HashMap<String, Incoming>>> = Mutex::new(None);

#[tauri::command(async)]
pub fn share_recv_begin(id: String) -> Result<(), String> {
    check_id(&id)?;
    std::fs::create_dir_all(share_dir()).map_err(|e| e.to_string())?;
    let file = std::fs::File::create(incoming_path(&id)).map_err(|e| e.to_string())?;
    INCOMING
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(id, Incoming { file, hasher: sha2::Sha256::new() });
    Ok(())
}

/// Appends received bytes (the raw body of the call; the share id comes in "x-share-id").
#[tauri::command(async)]
pub fn share_recv_chunk(request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let id = request.headers().get("x-share-id").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Se esperaban datos binarios".to_string());
    };
    let mut map = INCOMING.lock().unwrap_or_else(|e| e.into_inner());
    let incoming = map.as_mut().and_then(|m| m.get_mut(&id)).ok_or("No hay ninguna recepción en curso")?;
    incoming.file.write_all(bytes).map_err(|e| format!("No se pudo guardar lo recibido: {}", e))?;
    incoming.hasher.update(bytes);
    Ok(())
}

#[tauri::command(async)]
pub fn share_recv_abort(id: String) {
    if let Some(map) = INCOMING.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        map.remove(&id);
    }
    if check_id(&id).is_ok() {
        let _ = std::fs::remove_file(incoming_path(&id));
    }
}

/// The name the instance will show and the folder it goes in: the sender's name, with " (2)"...
/// while an instance of this launcher already uses either (the instance list hides duplicates
/// of both). The folder is the name without what Windows doesn't allow in a file name.
fn free_instance_name(wanted: &str) -> (String, String) {
    let folder_of = |name: &str| -> String {
        let clean: String = name.chars().filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') && !c.is_control()).collect();
        let clean = clean.trim().trim_end_matches('.').trim().to_string();
        if clean.is_empty() { "Instancia recibida".to_string() } else { clean }
    };
    let wanted: String = wanted.chars().filter(|c| !c.is_control()).collect();
    let base = if wanted.trim().is_empty() { "Instancia recibida" } else { wanted.trim() };
    let taken: std::collections::HashSet<String> = crate::get_all_instances()
        .into_iter()
        .flat_map(|i| [i.name.to_lowercase(), i.folder_name.to_lowercase()])
        .collect();
    let root = crate::mc::instances_root();
    let free = |name: &str| {
        let folder = folder_of(name);
        !taken.contains(&name.to_lowercase()) && !taken.contains(&folder.to_lowercase()) && !root.join(&folder).exists()
    };
    let name = std::iter::once(base.to_string())
        .chain((2..).map(|n| format!("{} ({})", base, n)))
        .find(|name| free(name))
        .unwrap_or_else(|| base.to_string());
    let folder = folder_of(&name);
    (name, folder)
}

fn unpack(archive: &Path, target: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive).map_err(|e| e.to_string())?)
        .map_err(|_| "Lo recibido no es un paquete de instancia válido".to_string())?;
    if zip.by_name("mmc-pack.json").is_err() {
        return Err("Lo recibido no es un paquete de instancia válido".to_string());
    }
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        // enclosed_name: nothing that would leave the instance folder
        let Some(rel) = entry.enclosed_name() else { continue };
        let mut parts = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string());
        let dest = match parts.next().as_deref() {
            Some("instance.cfg") => target.join("instance.cfg"),
            Some("mmc-pack.json") => target.join("mmc-pack.json"),
            Some("patches") => parts.fold(target.join("patches"), |p, c| p.join(c)),
            Some("minecraft") => parts.fold(target.join(".minecraft"), |p, c| p.join(c)),
            _ => continue,
        };
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("No se pudo extraer {}: {}", dest.display(), e))?;
    }
    Ok(())
}

/// Everything arrived: checks it is exactly what the sender packed and makes it a new instance
/// in the launcher's own folder. Returns the instance folder.
#[tauri::command(async)]
pub fn share_recv_finish(id: String, sha256: String, name: String) -> Result<String, String> {
    check_id(&id)?;
    let incoming = INCOMING.lock().unwrap_or_else(|e| e.into_inner()).as_mut().and_then(|m| m.remove(&id));
    let Some(incoming) = incoming else { return Err("No hay ninguna recepción en curso".to_string()) };
    let part = incoming_path(&id);
    let got: String = incoming.hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect();
    drop(incoming.file);
    if got != sha256.to_lowercase() {
        let _ = std::fs::remove_file(&part);
        return Err("La instancia llegó incompleta o dañada. Pide que te la envíen de nuevo.".to_string());
    }

    let (final_name, folder) = free_instance_name(&name);
    let target = crate::mc::instances_root().join(&folder);
    let result = unpack(&part, &target).and_then(|_| {
        std::fs::create_dir_all(target.join(".minecraft").join("mods")).map_err(|e| e.to_string())?;
        let mut set = vec![("name", final_name.clone())];
        // Memory set for the sender's PC may not fit this one.
        let cfg = crate::read_instance_cfg(&target);
        let wanted = cfg.iter().find(|(k, _)| k == "MaxMemAlloc").and_then(|(_, v)| v.parse::<u64>().ok());
        let safe = (crate::get_system_memory_mb() / 2).max(2048);
        if wanted.is_some_and(|mb| mb > safe) {
            set.push(("MaxMemAlloc", safe.to_string()));
        }
        crate::edit_instance_cfg(&target, &set, &[])
    });
    let _ = std::fs::remove_file(&part);
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&target);
        return Err(e);
    }
    Ok(target.to_string_lossy().to_string())
}
