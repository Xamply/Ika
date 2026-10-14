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
/// screenshots, logs or caches. Includes all mod configs, KubeJS, FancyMenu, PackMenu, scripts, options, servers.
const GAME_DIRS: [&str; 12] = [
    "mods",
    "config",
    "shaderpacks",
    "resourcepacks",
    "defaultconfigs",
    "kubejs",
    "fancymenu",
    "fancymenu_data",
    "packmenu",
    "custommenu",
    "scripts",
    "schematics",
];
const GAME_FILES: [&str; 6] = [
    "options.txt",
    "optionsshaders.txt",
    "optionsof.txt",
    "servers.dat",
    "servers.dat_old",
    "usercache.json",
];
const ROOT_EXTRA_FILES: [&str; 6] = [
    ".ika-cover.png",
    ".ika-cover.jpg",
    ".ika-cover.jpeg",
    ".ika-cover.webp",
    "icon.png",
    "instance.json",
];

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
    let trimmed = raw.trim();
    let only_digits_or_sep = !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit() || c == '-' || c == ' ');
    let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();
    if only_digits_or_sep && digits.len() == 6 {
        Ok(digits)
    } else {
        Err("Introduce el usuario de tu amigo (o un código de 6 dígitos).".to_string())
    }
}

fn user_to_code(name: &str) -> String {
    let clean = name.trim().trim_start_matches('@').to_lowercase();
    let hash = sha2::Sha256::digest(format!("ika-user-code:{}", clean).as_bytes());
    let n = u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]);
    format!("{:06}", 100_000 + (n % 900_000))
}

fn clean_username_query(raw: &str) -> Result<String, String> {
    let clean: String = raw
        .trim()
        .trim_start_matches('@')
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .collect();
    if (2..=24).contains(&clean.len()) {
        Ok(clean.to_lowercase())
    } else {
        Err("Escribe un nombre de usuario válido de tu amigo (mínimo 2 caracteres).".to_string())
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

fn explain_room(err: String, room_ref: Option<&str>) -> String {
    let (status, detail) = err.split_once('|').unwrap_or(("", err.as_str()));
    match (status, detail) {
        ("404", "room not found") => {
            if let Some(c) = room_ref {
                let is_digits = c.len() == 6 && c.chars().all(|ch| ch.is_ascii_digit());
                if is_digits {
                    format!("No hay ningún envío activo para este código o usuario (o ya expiró). Pídele a tu amigo que inicie el envío.")
                } else {
                    format!("No hay ningún envío activo de «{}» (o ya expiró). Pídele a tu amigo que pulse Enviar instancia.", c)
                }
            } else {
                "La transferencia ya expiró o fue cerrada.".to_string()
            }
        }
        ("403", "room already in use") => "Este envío ya está siendo recibido por otro jugador.".to_string(),
        ("410", _) => "El envío fue cancelado por el otro jugador.".to_string(),
        ("0", _) => "No se pudo conectar con el servicio de Ika. Revisa tu conexión a internet.".to_string(),
        _ => format!("El servicio de Ika respondió {}: {}", status, detail),
    }
}

/// Creates a 6-digit room for an outgoing instance transfer, linked under the hood to the username.
#[tauri::command(async)]
pub fn share_room_create(
    id: String,
    sender: String,
    recipient: Option<String>,
    meta: serde_json::Value,
    offer: Option<String>,
) -> Result<serde_json::Value, String> {
    check_id(&id)?;
    let sender_trimmed = sender.trim();
    let recipient_trimmed = recipient.as_deref().unwrap_or("").trim().trim_start_matches('@');
    let key_user = if !recipient_trimmed.is_empty() {
        recipient_trimmed
    } else if !sender_trimmed.is_empty() {
        sender_trimmed
    } else {
        &id
    };
    let preferred_code = user_to_code(key_user);
    let payload = serde_json::json!({
        "id": id,
        "sender": sender_trimmed,
        "recipient": recipient_trimmed,
        "code": preferred_code,
        "meta": meta,
        "offer": offer,
    });
    let body = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
    mailbox("POST", "/share/room", &body).map_err(|e| explain_room(e, None))
}

/// Reads the current state of a room by 6-digit code OR by player username (which resolves to the 6-digit code under the hood).
#[tauri::command(async)]
pub fn share_room_get(code: String) -> Result<serde_json::Value, String> {
    if let Ok(digits) = valid_code(&code) {
        return mailbox("GET", &format!("/share/room/{}", digits), &[]).map_err(|e| explain_room(e, Some(&digits)));
    }
    let user = clean_username_query(&code)?;
    let by_user_path = format!("/share/room/by-user/{}", user);
    mailbox("GET", &by_user_path, &[]).map_err(|e| explain_room(e, Some(code.trim())))
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

/// Downloads a chunk from the room's relay fallback and writes it to the incoming file (supports parallel out-of-order writes via `offset`).
#[tauri::command(async)]
pub fn share_room_get_chunk(code: String, id: String, index: u32, offset: Option<u64>) -> Result<serde_json::Value, String> {
    let code = valid_code(&code)?;
    check_id(&id)?;
    match mailbox_raw("GET", &format!("/share/room/{}/chunk/{}", code, index), &[]) {
        Ok(bytes) => {
            let mut map = INCOMING.lock().unwrap_or_else(|e| e.into_inner());
            let incoming = map.as_mut().and_then(|m| m.get_mut(&id)).ok_or("No hay ninguna recepción en curso")?;
            if let Some(off) = offset {
                incoming.positional = true;
                incoming.file.seek(SeekFrom::Start(off)).map_err(|e| format!("No se pudo posicionar lo recibido: {}", e))?;
                incoming.file.write_all(&bytes).map_err(|e| format!("No se pudo guardar lo recibido: {}", e))?;
            } else {
                incoming.file.write_all(&bytes).map_err(|e| format!("No se pudo guardar lo recibido: {}", e))?;
                incoming.hasher.update(&bytes);
            }
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
// Sending: the package (with Smart Hash Deduplication & Streaming SHA-256)
// ---------------------------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct Package {
    /// Total unified weight (zip_size + cdn_size) shown to both sender and receiver.
    size: u64,
    /// Actual byte size of out-{id}.zip sent over WebRTC/Relay.
    zip_size: u64,
    /// Bytes deduplicated via official CDN hashes.
    cdn_size: u64,
    cdn_count: u32,
    sha256: String,
    files: u32,
    mods: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CdnModEntry {
    pub rel: String,
    pub url: String,
    pub sha1: String,
    pub size: u64,
}

fn sha1_of_file(path: &Path) -> Option<(String, u64)> {
    use sha1::Digest;
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len == 0 {
        return None;
    }
    let mut hasher = sha1::Sha1::new();
    let mut buf = vec![0u8; 128 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let hex: String = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect();
    Some((hex, len))
}

/// Scans `mods/`, `resourcepacks/` and `shaderpacks/` in parallel and queries Modrinth's batch
/// hash API (`POST /v2/version_files`) to separate files into:
/// - Group A: Exact SHA-1 match hosted on Modrinth CDN (returned in map `rel_path -> CdnModEntry`)
/// - Group B: Custom / non-Modrinth files (not in map, packed directly into the .zip)
fn resolve_cdn_files(game: &Path) -> HashMap<String, CdnModEntry> {
    use rayon::prelude::*;
    let mut candidates: Vec<(PathBuf, String)> = Vec::new();
    for dir_name in ["mods", "resourcepacks", "shaderpacks"] {
        let dir = game.join(dir_name);
        if !dir.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&dir).max_depth(2).follow_links(false).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let p = entry.path();
            let name_lower = p.file_name().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
            if !(name_lower.ends_with(".jar") || name_lower.ends_with(".jar.disabled") || name_lower.ends_with(".zip")) {
                continue;
            }
            let Ok(rel) = p.strip_prefix(&dir) else { continue };
            let rel_str = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            candidates.push((p.to_path_buf(), format!("minecraft/{}/{}", dir_name, rel_str)));
        }
    }
    if candidates.is_empty() {
        return HashMap::new();
    }

    // Compute SHA-1 across all CPU cores in parallel
    let hashed: Vec<(String, String, u64)> = candidates
        .into_par_iter()
        .filter_map(|(path, rel)| {
            let (sha1, size) = sha1_of_file(&path)?;
            Some((rel, sha1, size))
        })
        .collect();
    if hashed.is_empty() {
        return HashMap::new();
    }

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(3))
        .timeout_read(std::time::Duration::from_secs(5))
        .timeout_write(std::time::Duration::from_secs(4))
        .user_agent("IkaLauncher/1.5")
        .build();

    let mut matched_urls: HashMap<String, String> = HashMap::new();
    let all_hashes: Vec<String> = hashed.iter().map(|(_, h, _)| h.clone()).collect();
    for chunk in all_hashes.chunks(350) {
        let body = serde_json::json!({
            "hashes": chunk,
            "algorithm": "sha1"
        });
        let Ok(resp) = agent
            .post("https://api.modrinth.com/v2/version_files")
            .set("content-type", "application/json")
            .send_string(&body.to_string())
        else {
            continue;
        };
        let Ok(text) = resp.into_string() else { continue };
        let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&text) else { continue };
        for (queried_sha1, ver_obj) in map {
            let Some(files) = ver_obj.get("files").and_then(|f| f.as_array()) else { continue };
            let q_lower = queried_sha1.to_ascii_lowercase();
            if let Some(url) = files.iter().find_map(|f| {
                let f_sha1 = f.pointer("/hashes/sha1").and_then(|s| s.as_str())?.to_ascii_lowercase();
                let url = f.get("url").and_then(|u| u.as_str())?;
                (f_sha1 == q_lower && url.starts_with("https://cdn.modrinth.com/")).then(|| url.to_string())
            }) {
                matched_urls.insert(q_lower, url);
            }
        }
    }

    let mut out = HashMap::new();
    for (rel, sha1, size) in hashed {
        let sha1_lower = sha1.to_ascii_lowercase();
        if let Some(url) = matched_urls.get(&sha1_lower) {
            out.insert(
                rel.clone(),
                CdnModEntry {
                    rel,
                    url: url.clone(),
                    sha1: sha1_lower,
                    size,
                },
            );
        }
    }
    out
}

fn add_file<W: Write + Seek>(zip: &mut zip::ZipWriter<W>, path: &Path, name: &str) -> Result<(), String> {
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
    let file = std::fs::File::open(path).map_err(|e| format!("No se pudo leer {}: {}", path.display(), e))?;
    let mut reader = std::io::BufReader::with_capacity(256 * 1024, file);
    zip.start_file(name, options).map_err(|e| e.to_string())?;
    std::io::copy(&mut reader, zip).map_err(|e| format!("No se pudo empaquetar {}: {}", path.display(), e))?;
    Ok(())
}

/// Adds a folder; skips files that belong to Group A (`cdn_map`) so only custom/non-CDN files are packed into the zip.
fn add_dir<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    dir: &Path,
    prefix: &str,
    cdn_map: &HashMap<String, CdnModEntry>,
    used_cdn: &mut Vec<CdnModEntry>,
) -> Result<u32, String> {
    let mut count = 0;
    for entry in walkdir::WalkDir::new(dir).follow_links(false).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(dir) else { continue };
        let rel = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        let full_rel = format!("{}/{}", prefix, rel);
        if let Some(cdn_entry) = cdn_map.get(&full_rel) {
            used_cdn.push(cdn_entry.clone());
            count += 1;
            continue;
        }
        add_file(zip, entry.path(), &full_rel)?;
        count += 1;
    }
    Ok(count)
}

/// Builds the package of an instance in the share folder as out-{id}.zip:
/// Automatically splits files into Group A (exact SHA-1 CDN matches stored in `ika-cdn.json`)
/// and Group B (custom mods + all exact configs, KubeJS, FancyMenu, options, servers packed in the zip).
/// Returns the unified total weight (`size = zip_size + cdn_size`) so the user sees a single total size.
#[tauri::command(async)]
pub fn share_pack(instance_path: String, id: String) -> Result<Package, String> {
    check_id(&id)?;
    let root = crate::instance_root_dir(&instance_path)?;
    let game = game_dir(&root);
    std::fs::create_dir_all(share_dir()).map_err(|e| e.to_string())?;
    let out = outgoing_path(&id);

    // Resolve Group A (CDN-matched files by exact SHA-1) vs Group B (custom files + configs)
    let cdn_map = resolve_cdn_files(&game);
    let mut used_cdn: Vec<CdnModEntry> = Vec::new();
    let empty_cdn: HashMap<String, CdnModEntry> = HashMap::new();

    let result = (|| -> Result<(u32, u32), String> {
        let raw_file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        let buf_writer = std::io::BufWriter::with_capacity(4 * 1024 * 1024, raw_file);
        let mut zip = zip::ZipWriter::new(buf_writer);
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
            files += add_dir(&mut zip, &root.join("patches"), "patches", &empty_cdn, &mut used_cdn)?;
        }
        if root.join("jarmods").is_dir() {
            files += add_dir(&mut zip, &root.join("jarmods"), "jarmods", &empty_cdn, &mut used_cdn)?;
        }
        for extra in ROOT_EXTRA_FILES {
            if root.join(extra).is_file() {
                add_file(&mut zip, &root.join(extra), extra)?;
                files += 1;
            }
        }
        for dir in GAME_DIRS {
            if game.join(dir).is_dir() {
                files += add_dir(&mut zip, &game.join(dir), &format!("minecraft/{}", dir), &cdn_map, &mut used_cdn)?;
            }
        }
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
        if !used_cdn.is_empty() {
            let cdn_json = serde_json::to_vec(&used_cdn).map_err(|e| e.to_string())?;
            zip.start_file("ika-cdn.json", text).map_err(|e| e.to_string())?;
            zip.write_all(&cdn_json).map_err(|e| e.to_string())?;
        }
        let mut buf_writer = zip.finish().map_err(|e| e.to_string())?;
        buf_writer.flush().map_err(|e| e.to_string())?;
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
    let file = std::fs::File::open(&out).map_err(|e| e.to_string())?;
    let mut reader = std::io::BufReader::with_capacity(4 * 1024 * 1024, file);
    std::io::copy(&mut reader, &mut hasher).map_err(|e| e.to_string())?;
    let zip_size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    let cdn_size: u64 = used_cdn.iter().map(|c| c.size).sum();
    let cdn_count = used_cdn.len() as u32;
    Ok(Package {
        size: zip_size + cdn_size,
        zip_size,
        cdn_size,
        cdn_count,
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
    positional: bool,
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
        .insert(
            id,
            Incoming {
                file,
                hasher: sha2::Sha256::new(),
                positional: false,
            },
        );
    Ok(())
}

/// Appends received bytes (the raw body of the call; the share id comes in "x-share-id",
/// and optional byte offset comes in "x-share-offset" for multi-channel parallel WebRTC).
#[tauri::command(async)]
pub fn share_recv_chunk(request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let id = request.headers().get("x-share-id").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
    let offset = request
        .headers()
        .get("x-share-offset")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Se esperaban datos binarios".to_string());
    };
    let mut map = INCOMING.lock().unwrap_or_else(|e| e.into_inner());
    let incoming = map.as_mut().and_then(|m| m.get_mut(&id)).ok_or("No hay ninguna recepción en curso")?;
    if let Some(off) = offset {
        incoming.positional = true;
        incoming.file.seek(SeekFrom::Start(off)).map_err(|e| format!("No se pudo posicionar lo recibido: {}", e))?;
        incoming.file.write_all(bytes).map_err(|e| format!("No se pudo guardar lo recibido: {}", e))?;
    } else {
        incoming.file.write_all(bytes).map_err(|e| format!("No se pudo guardar lo recibido: {}", e))?;
        incoming.hasher.update(bytes);
    }
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

fn unpack(archive: &Path, target: &Path) -> Result<Vec<CdnModEntry>, String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let reader = std::io::BufReader::with_capacity(2 * 1024 * 1024, file);
    let mut zip = zip::ZipArchive::new(reader)
        .map_err(|_| "Lo recibido no es un paquete de instancia válido".to_string())?;
    if zip.by_name("mmc-pack.json").is_err() {
        return Err("Lo recibido no es un paquete de instancia válido".to_string());
    }
    let mut cdn_entries: Vec<CdnModEntry> = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let Some(rel) = entry.enclosed_name() else { continue };
        let mut parts = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string());
        let first = parts.next();
        if first.as_deref() == Some("ika-cdn.json") {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            if let Ok(parsed) = serde_json::from_slice::<Vec<CdnModEntry>>(&buf) {
                cdn_entries = parsed;
            }
            continue;
        }
        let dest = match first.as_deref() {
            Some("instance.cfg") => target.join("instance.cfg"),
            Some("mmc-pack.json") => target.join("mmc-pack.json"),
            Some("patches") => parts.fold(target.join("patches"), |p, c| p.join(c)),
            Some("jarmods") => parts.fold(target.join("jarmods"), |p, c| p.join(c)),
            Some("minecraft") => parts.fold(target.join(".minecraft"), |p, c| p.join(c)),
            Some(extra) if ROOT_EXTRA_FILES.contains(&extra) => target.join(extra),
            _ => continue,
        };
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let out_file = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
        let mut out = std::io::BufWriter::with_capacity(256 * 1024, out_file);
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("No se pudo extraer {}: {}", dest.display(), e))?;
        out.flush().map_err(|e| e.to_string())?;
    }
    Ok(cdn_entries)
}

/// Downloads all Group A CDN-deduplicated files in parallel (16 threads) with SHA-1 verification,
/// emitting real-time byte progress (`share-transfer-progress`) so the receiver sees a single unified progress bar.
fn download_cdn_entries(
    app: &tauri::AppHandle,
    id: &str,
    target: &Path,
    entries: &[CdnModEntry],
) -> Result<(), String> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tauri::Emitter;

    if entries.is_empty() {
        return Ok(());
    }
    let total_bytes: u64 = entries.iter().map(|e| e.size).sum();
    let done_bytes = AtomicU64::new(0);
    let last_emit = Mutex::new(std::time::Instant::now());
    let errors = Mutex::new(Vec::<String>::new());

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(16)
        .build()
        .map_err(|e| e.to_string())?;

    pool.install(|| {
        entries.par_iter().for_each_init(
            || {
                ureq::AgentBuilder::new()
                    .timeout_connect(std::time::Duration::from_secs(12))
                    .timeout_read(std::time::Duration::from_secs(45))
                    .user_agent("IkaLauncher/1.5")
                    .build()
            },
            |agent, item| {
                let Some(rel_clean) = Path::new(&item.rel).components().try_fold(PathBuf::new(), |mut acc, c| {
                    match c {
                        std::path::Component::Normal(s) => {
                            acc.push(s);
                            Some(acc)
                        }
                        _ => None,
                    }
                }) else {
                    return;
                };
                let mut parts = rel_clean.components().map(|c| c.as_os_str().to_string_lossy().to_string());
                let dest = match parts.next().as_deref() {
                    Some("minecraft") => parts.fold(target.join(".minecraft"), |p, c| p.join(c)),
                    _ => return,
                };
                if let Some(parent) = dest.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }

                let mut ok = false;
                let mut last_err = String::new();
                for attempt in 0..3 {
                    if attempt > 0 {
                        std::thread::sleep(std::time::Duration::from_millis(400 * attempt));
                    }
                    match agent.get(&item.url).call() {
                        Ok(resp) => {
                            let mut bytes = Vec::with_capacity(item.size as usize);
                            if let Err(e) = resp.into_reader().read_to_end(&mut bytes) {
                                last_err = format!("{}: {}", item.url, e);
                                continue;
                            }
                            let got_sha1: String = {
                                use sha1::Digest;
                                let mut h = sha1::Sha1::new();
                                h.update(&bytes);
                                h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
                            };
                            if !got_sha1.eq_ignore_ascii_case(&item.sha1) {
                                last_err = format!("{}: SHA-1 no coincide", item.rel);
                                continue;
                            }
                            if let Err(e) = std::fs::write(&dest, &bytes) {
                                last_err = format!("{}: {}", dest.display(), e);
                                continue;
                            }
                            ok = true;
                            break;
                        }
                        Err(e) => {
                            last_err = format!("{}: {}", item.url, e);
                        }
                    }
                }
                if !ok {
                    errors.lock().unwrap_or_else(|e| e.into_inner()).push(last_err);
                    return;
                }
                let cur = done_bytes.fetch_add(item.size, Ordering::SeqCst) + item.size;
                let mut last = last_emit.lock().unwrap_or_else(|e| e.into_inner());
                if cur >= total_bytes || last.elapsed().as_millis() >= 80 {
                    *last = std::time::Instant::now();
                    let _ = app.emit(
                        "share-transfer-progress",
                        serde_json::json!({
                            "id": id,
                            "stage": "cdn",
                            "cdnDone": cur,
                            "cdnTotal": total_bytes,
                        }),
                    );
                }
            },
        );
    });

    let errs = errors.into_inner().unwrap_or_default();
    if !errs.is_empty() {
        return Err(format!(
            "No se pudieron descargar {} mod(s) desde el CDN:\n{}",
            errs.len(),
            errs.into_iter().take(3).collect::<Vec<_>>().join("\n")
        ));
    }
    Ok(())
}

/// Everything arrived: checks it is exactly what the sender packed, downloads Group A CDN mods
/// at full speed, and pre-installs the Minecraft runtime/loader/assets/Java in one single flow.
#[tauri::command(async)]
pub fn share_recv_finish(app: tauri::AppHandle, id: String, sha256: String, name: String) -> Result<String, String> {
    use tauri::Emitter;
    check_id(&id)?;
    let incoming = INCOMING.lock().unwrap_or_else(|e| e.into_inner()).as_mut().and_then(|m| m.remove(&id));
    let Some(mut incoming) = incoming else { return Err("No hay ninguna recepción en curso".to_string()) };
    let part = incoming_path(&id);
    let _ = incoming.file.flush();
    let got: String = if incoming.positional {
        drop(incoming.file);
        let f = std::fs::File::open(&part).map_err(|e| e.to_string())?;
        let mut reader = std::io::BufReader::with_capacity(4 * 1024 * 1024, f);
        let mut h = sha2::Sha256::new();
        std::io::copy(&mut reader, &mut h).map_err(|e| e.to_string())?;
        h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
    } else {
        drop(incoming.file);
        incoming.hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect()
    };
    if got != sha256.to_lowercase() {
        let _ = std::fs::remove_file(&part);
        return Err("La instancia llegó incompleta o dañada. Pide que te la envíen de nuevo.".to_string());
    }

    let (final_name, folder) = free_instance_name(&name);
    let target = crate::mc::instances_root().join(&folder);
    let result = unpack(&part, &target).and_then(|cdn_entries| {
        std::fs::create_dir_all(target.join(".minecraft").join("mods")).map_err(|e| e.to_string())?;
        let mut set = vec![("name", final_name.clone())];
        // Memory set for the sender's PC may not fit this one.
        let cfg = crate::read_instance_cfg(&target);
        let wanted = cfg.iter().find(|(k, _)| k == "MaxMemAlloc").and_then(|(_, v)| v.parse::<u64>().ok());
        let safe = (crate::get_system_memory_mb() / 2).max(2048);
        if wanted.is_some_and(|mb| mb > safe) {
            set.push(("MaxMemAlloc", safe.to_string()));
        }
        crate::edit_instance_cfg(&target, &set, &[])?;

        // Step 2: Download Group A CDN mods in parallel (16 threads) as part of the single unified download
        if !cdn_entries.is_empty() {
            download_cdn_entries(&app, &id, &target, &cdn_entries)?;
        }

        // Step 3: Pre-install Minecraft version, Modloader libraries, assets, and Java runtime
        // so clicking "Jugar ahora" launches immediately without a second download!
        let _ = app.emit(
            "share-transfer-progress",
            serde_json::json!({
                "id": id,
                "stage": "runtime",
                "message": "Preparando Minecraft, librerías del loader y Java...",
            }),
        );
        crate::mc::preinstall_instance_runtime(Some(app.clone()), &target)?;
        Ok(())
    });
    let _ = std::fs::remove_file(&part);
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&target);
        return Err(e);
    }
    Ok(target.to_string_lossy().to_string())
}
