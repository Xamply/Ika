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

/// What stays on the PC the instance was played on: worlds, screenshots, logs, crash reports,
/// backups, caches, the map data of map mods (their settings do travel) and what mods rebuild by
/// themselves. Everything else in the game folder travels, whatever folder a mod or modpack
/// keeps it in (RLCraft's resources/ and structures/, for one), so the instance is the same one.
const STAYS_HOME_DIRS: [&str; 18] = [
    "saves",
    "screenshots",
    "logs",
    "crash-reports",
    "backups",
    "simplebackups",
    "downloads",
    "cache",
    ".cache",
    ".fabric",
    ".mixin.out",
    "modernfix",
    "showdown",
    "assets",
    "libraries",
    "versions",
    "natives",
    "debug",
];
/// Paths inside a folder that travels (map data of map mods, caches of mod loaders).
const STAYS_HOME_PATHS: [&str; 4] = ["journeymap/data/", "mods/.connector/", "mods/.index/", "config/.cache/"];
const STAYS_HOME_FILES: [&str; 3] = ["usernamecache.json", "realms_persistence.json", "command_history.txt"];
const STAYS_HOME_SUFFIXES: [&str; 5] = [".log", ".log.gz", ".tmp", ".lck", ".pid"];

/// Whether `rel` (a path inside the game folder, with '/') stays on this PC.
fn stays_home(rel: &str) -> bool {
    let rel = rel.to_ascii_lowercase();
    let top = rel.split('/').next().unwrap_or("");
    // Xaero's minimap / world map keep their waypoints and maps in xaero*/ folders
    STAYS_HOME_DIRS.contains(&top)
        || top.starts_with("xaero")
        || STAYS_HOME_PATHS.iter().any(|p| rel.starts_with(p))
        || (!rel.contains('/') && STAYS_HOME_FILES.contains(&rel.as_str()))
        || STAYS_HOME_SUFFIXES.iter().any(|s| rel.ends_with(s))
        || rel.contains("/hs_err_pid")
}

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
    let signature = key.sign(format!("{}\n{}\n{}\n{}\n{}", crate::device_key::signed_prefix("ika-share-v1"), method, path, ts, digest).as_bytes());
    let request = http_agent()
        .request(method, &format!("{}{}", service, path))
        .set("x-ika-pubkey", &B64.encode(key.verifying_key().to_bytes()))
        .set("x-ika-timestamp", &ts)
        .set("x-ika-version", crate::device_key::CLIENT_VERSION)
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
    let signature = key.sign(format!("{}\n{}\n{}\n{}\n{}", crate::device_key::signed_prefix("ika-share-v1"), method, path, ts, digest).as_bytes());
    let request = http_agent()
        .request(method, &format!("{}{}", service, path))
        .set("x-ika-pubkey", &B64.encode(key.verifying_key().to_bytes()))
        .set("x-ika-timestamp", &ts)
        .set("x-ika-version", crate::device_key::CLIENT_VERSION)
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
/// What a launcher older than the service accepts is told (426 from the service).
const UPDATE_REQUIRED: &str = "Esta versión de Ika ya no es compatible con el servicio. Actualiza el launcher a la última versión para seguir usando amigos, grupos y skins.";

fn explain(err: String, me: &str, other: Option<&str>) -> String {
    let (code, detail) = err.split_once('|').unwrap_or(("", err.as_str()));
    match (code, detail) {
        ("426", _) => UPDATE_REQUIRED.to_string(),
        ("404", "unknown recipient") => format!(
            "No existe ningún jugador «{}» en Ika. Tu amigo tiene que abrir su launcher con ese nombre al menos una vez.",
            other.unwrap_or("")
        ),
        ("403", "blocked") => "Esta cuenta está bloqueada en el servicio de Ika.".to_string(),
        ("403", "not human") => "La verificación de Cloudflare no se completó. Vuelve a intentarlo.".to_string(),
        ("428", _) => "Tu perfil de Ika aún no está creado. Abre Amigos para crearlo.".to_string(),
        ("400", "bad work") => "No se pudo verificar este equipo. Inténtalo de nuevo.".to_string(),
        ("403", _) => format!("El nombre «{}» ya pertenece a otro PC: usa otro nombre de jugador para enviar y recibir instancias.", me),
        ("429", "too many new names") => "Desde esta conexión ya se crearon demasiados nombres hoy. Inténtalo mañana.".to_string(),
        ("429", _) => "Vas demasiado rápido. Espera un minuto e inténtalo de nuevo.".to_string(),
        ("0", _) => "No se pudo conectar con el servicio de Ika. Revisa tu conexión a internet.".to_string(),
        _ => format!("El servicio de Ika respondió {}: {}", code, detail),
    }
}

fn explain_room(err: String, room_ref: Option<&str>) -> String {
    let (status, detail) = err.split_once('|').unwrap_or(("", err.as_str()));
    match (status, detail) {
        ("426", _) => UPDATE_REQUIRED.to_string(),
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
        ("429", _) => "Vas demasiado rápido. Espera un minuto e inténtalo de nuevo.".to_string(),
        ("403", "blocked") => "Esta cuenta está bloqueada en el servicio de Ika.".to_string(),
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

/// The work the service asks for before creating a profile or a group: a nonce whose SHA-256 of
/// "ika-pow:{kind}:{pubkey}:{subject}:{day}:{nonce}" starts with `bits` zero bits (about a second
/// here; the service checks it with one hash).
fn proof_of_work(kind: &str, subject: &str, bits: u32) -> Result<u64, String> {
    let key = device_key()?;
    let pubkey = B64.encode(key.verifying_key().to_bytes());
    let day = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() / 86_400_000)
        .unwrap_or(0);
    let prefix = format!("ika-pow:{}:{}:{}:{}:", kind, pubkey, subject, day);
    let base = sha2::Sha256::new_with_prefix(prefix.as_bytes());
    let zero_bits = |hash: &[u8]| {
        let mut n = 0;
        for b in hash {
            if *b == 0 {
                n += 8;
                continue;
            }
            return n + b.leading_zeros();
        }
        n
    };
    for nonce in 0u64.. {
        let hash = base.clone().chain_update(nonce.to_string().as_bytes()).finalize();
        if zero_bits(&hash) >= bits {
            return Ok(nonce);
        }
    }
    Err("No se pudo completar la verificación".to_string())
}

/// Whether this PC's player name exists on the service: "mine" (this PC's), "taken" (another
/// PC's) or "free" (not created yet: claim it with name_claim).
#[tauri::command(async)]
pub fn name_status(username: String) -> Result<String, String> {
    let me = valid_name(&username)?;
    let res = mailbox("GET", &format!("/names/status?name={}", me), &[]).map_err(|e| explain(e, &me, None))?;
    Ok(res.get("state").and_then(|s| s.as_str()).unwrap_or("free").to_string())
}

/// Creates this PC's profile on the service: the Cloudflare check passed in the launcher
/// (`turnstile`, empty while the service doesn't ask for it) and the work done here.
#[tauri::command(async)]
pub fn name_claim(username: String, turnstile: Option<String>) -> Result<(), String> {
    let me = valid_name(&username)?;
    let nonce = proof_of_work("claim", &me, 22)?;
    let body = serde_json::to_vec(&serde_json::json!({ "turnstile": turnstile.unwrap_or_default(), "nonce": nonce.to_string() })).map_err(|e| e.to_string())?;
    mailbox("POST", &format!("/names/claim?name={}", me), &body).map(|_| ()).map_err(|e| explain(e, &me, None))
}

/// Address of this player's live connection (the service's Hub), signed with the device key. A
/// browser WebSocket cannot send headers, so the signature goes in the query; it covers
/// "/hub/connect?name={me}" and, like every signed call, is only accepted for a few minutes.
#[tauri::command(async)]
pub fn social_hub_url(username: String) -> Result<String, String> {
    let me = valid_name(&username)?;
    let service = crate::device_key::skins_service_url().ok_or("El servicio de Ika no está configurado")?;
    let key = device_key()?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
        .to_string();
    let path = format!("/hub/connect?name={}", me);
    let digest: String = sha2::Sha256::digest(b"").iter().map(|b| format!("{:02x}", b)).collect();
    let signature = key.sign(format!("{}\nGET\n{}\n{}\n{}", crate::device_key::signed_prefix("ika-share-v1"), path, ts, digest).as_bytes());
    let encode = |s: String| s.replace('+', "%2B").replace('/', "%2F").replace('=', "%3D");
    let ws_service = if let Some(rest) = service.strip_prefix("https://") {
        format!("wss://{}", rest)
    } else {
        format!("ws://{}", service.trim_start_matches("http://"))
    };
    Ok(format!(
        "{}{}&pub={}&ts={}&sig={}&v={}",
        ws_service,
        path,
        encode(B64.encode(key.verifying_key().to_bytes())),
        ts,
        encode(B64.encode(signature.to_bytes())),
        crate::device_key::CLIENT_VERSION
    ))
}

/// Friends, friend requests, mailbox and the invitation waiting for this player, in one call:
/// what the live connection sends when it opens, for when it cannot be opened.
#[tauri::command(async)]
pub fn social_sync(username: String) -> Result<serde_json::Value, String> {
    let me = valid_name(&username)?;
    mailbox("GET", &format!("/social/sync?name={}", me), &[]).map_err(|e| explain(e, &me, None))
}

/// request | accept | decline | cancel | remove, towards another player. Answers this player's
/// friends and pending requests as they are afterwards.
#[tauri::command(async)]
pub fn social_friend(username: String, action: String, who: String) -> Result<serde_json::Value, String> {
    let me = valid_name(&username)?;
    let who = valid_name(&who)?;
    if who == me {
        return Err("No puedes añadirte a ti mismo como amigo.".to_string());
    }
    let body = serde_json::to_vec(&serde_json::json!({ "action": action, "who": who })).map_err(|e| e.to_string())?;
    mailbox("POST", &format!("/social/friend?name={}", me), &body).map_err(|e| {
        let (code, detail) = e.split_once('|').unwrap_or(("", e.as_str()));
        match (code, detail) {
            ("409", "too many friends") => "Has llegado al máximo de 200 amigos.".to_string(),
            ("409", "too many requests") => "Tienes demasiadas solicitudes sin responder. Cancela alguna antes de enviar otra.".to_string(),
            ("404", "no friend request") => format!("La solicitud de «{}» ya no existe.", who),
            _ => explain(e.clone(), &me, Some(&who)),
        }
    })
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
    /// Other places with exactly this file (CurseForge and its mirrors, Modrinth), tried in
    /// order when `url` fails. Launchers that don't know the field just use `url`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alts: Vec<String>,
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
/// Download links on CurseForge's CDN for files it hosts, by their fingerprint (what the
/// CurseForge app matches mods with), keyed by sha1: its own link and its two mirrors. Only
/// files whose sha1 CurseForge also reports are kept, so the links are for exactly this file.
fn curseforge_urls(files: &[&(String, String, u64, PathBuf)]) -> HashMap<String, Vec<String>> {
    use rayon::prelude::*;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(5))
        .timeout_read(std::time::Duration::from_secs(25))
        .user_agent("IkaLauncher/1.5")
        .build();
    let mut urls = HashMap::new();
    let prints: Vec<(u32, String)> = files
        .par_iter()
        .filter_map(|(_, sha1, _, path)| Some((crate::manager::cf_fingerprint(path.to_string_lossy().to_string()).ok()?, sha1.to_ascii_lowercase())))
        .collect();
    let wanted: HashMap<u32, String> = prints.iter().cloned().collect();
    for chunk in prints.chunks(500) {
        let body = serde_json::json!({ "fingerprints": chunk.iter().map(|(fp, _)| fp).collect::<Vec<_>>() });
        let Ok(resp) = agent
            .post("https://api.curseforge.com/v1/fingerprints/432")
            .set("x-api-key", crate::mc::CF_API_KEY)
            .set("content-type", "application/json")
            .send_string(&body.to_string())
        else {
            continue;
        };
        let Some(found) = resp.into_string().ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { continue };
        for m in found.pointer("/data/exactMatches").and_then(|v| v.as_array()).into_iter().flatten() {
            let Some(file) = m.get("file") else { continue };
            let Some(fp) = file.get("fileFingerprint").and_then(|v| v.as_u64()) else { continue };
            let Some(sha1) = wanted.get(&(fp as u32)) else { continue };
            let reported = file
                .get("hashes")
                .and_then(|h| h.as_array())
                .and_then(|arr| arr.iter().find(|h| h.get("algo").and_then(|a| a.as_u64()) == Some(1)))
                .and_then(|h| h.get("value").and_then(|v| v.as_str()))
                .map(|v| v.to_ascii_lowercase());
            if reported.as_deref() != Some(sha1.as_str()) {
                continue;
            }
            let id = file.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
            let name = file.get("fileName").and_then(|v| v.as_str()).unwrap_or("");
            let mut links: Vec<String> = file.get("downloadUrl").and_then(|v| v.as_str()).filter(|u| !u.is_empty()).map(str::to_string).into_iter().collect();
            if id > 0 && !name.is_empty() {
                for host in ["edge.forgecdn.net", "mediafilez.forgecdn.net"] {
                    let mirror = crate::mc::cf_cdn_url(host, id, name);
                    if !links.contains(&mirror) {
                        links.push(mirror);
                    }
                }
            }
            if !links.is_empty() {
                urls.insert(sha1.clone(), links);
            }
        }
    }
    urls
}

/// Where each file can be downloaded, by its sha1: Modrinth's CDN first (exact sha1 match), then
/// CurseForge's (exact fingerprint match, see curseforge_urls). Files on neither are left out.
fn cdn_links(hashed: &[(String, String, u64, PathBuf)]) -> HashMap<String, Vec<String>> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(3))
        .timeout_read(std::time::Duration::from_secs(5))
        .timeout_write(std::time::Duration::from_secs(4))
        .user_agent("IkaLauncher/1.5")
        .build();

    let mut matched_urls: HashMap<String, String> = HashMap::new();
    let all_hashes: Vec<String> = hashed.iter().map(|(_, h, _, _)| h.clone()).collect();
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

    // CurseForge (found by its fingerprint) has what Modrinth doesn't, and is a second source
    // for what both have: a download falls back to it when the first link fails
    let everything: Vec<&(String, String, u64, PathBuf)> = hashed.iter().collect();
    let on_curseforge = curseforge_urls(&everything);
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for (_, sha1, _, _) in hashed {
        let sha1_lower = sha1.to_ascii_lowercase();
        let mut links: Vec<String> = matched_urls.get(&sha1_lower).cloned().into_iter().collect();
        links.extend(on_curseforge.get(&sha1_lower).cloned().unwrap_or_default());
        if !links.is_empty() {
            out.insert(sha1_lower, links);
        }
    }
    out

}

fn resolve_cdn_files(game: &Path) -> HashMap<String, CdnModEntry> {
    use rayon::prelude::*;
    let mut candidates: Vec<(PathBuf, String)> = Vec::new();
    let dirs: Vec<String> = std::fs::read_dir(game)
        .map(|r| r.flatten().filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false)).map(|e| e.file_name().to_string_lossy().to_string()).collect())
        .unwrap_or_default();
    for dir_name in dirs.iter().filter(|d| !stays_home(d)) {
        let dir = game.join(dir_name);
        for entry in walkdir::WalkDir::new(&dir).max_depth(3).follow_links(false).into_iter().flatten() {
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
    let hashed: Vec<(String, String, u64, PathBuf)> = candidates
        .into_par_iter()
        .filter_map(|(path, rel)| {
            let (sha1, size) = sha1_of_file(&path)?;
            Some((rel, sha1, size, path))
        })
        .collect();
    if hashed.is_empty() {
        return HashMap::new();
    }

    let links_by_sha1 = cdn_links(&hashed);

    let mut out = HashMap::new();
    for (rel, sha1, size, _) in hashed {
        let sha1_lower = sha1.to_ascii_lowercase();
        let mut links = links_by_sha1.get(&sha1_lower).cloned().unwrap_or_default();
        if links.is_empty() {
            continue;
        }
        let url = links.remove(0);
        out.insert(
            rel.clone(),
            CdnModEntry {
                rel,
                url,
                sha1: sha1_lower,
                size,
                alts: links,
            },
        );
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
    skip: &std::collections::HashSet<String>,
) -> Result<u32, String> {
    let mut count = 0;
    for entry in walkdir::WalkDir::new(dir).follow_links(false).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(dir) else { continue };
        let rel = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
        let full_rel = format!("{}/{}", prefix, rel);
        if full_rel.strip_prefix("minecraft/").is_some_and(stays_home) {
            continue;
        }
        if let Some(cdn_entry) = cdn_map.get(&full_rel) {
            used_cdn.push(cdn_entry.clone());
            count += 1;
            continue;
        }
        if skip.contains(&full_rel) {
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
pub fn share_pack(app: tauri::AppHandle, instance_path: String, id: String, base: Option<bool>) -> Result<Package, String> {
    check_id(&id)?;
    let root = crate::instance_root_dir(&instance_path)?;
    let game = game_dir(&root);
    std::fs::create_dir_all(share_dir()).map_err(|e| e.to_string())?;
    let out = outgoing_path(&id);

    // For a group: files identical to the modpack the instance was installed from are left out;
    // followers take them from that modpack (see GroupBase). Not for a friend, whose launcher
    // may be older.
    let base = if base == Some(true) { group_base(&app, &id, &root) } else { None };
    let mut skip: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some((_, overrides)) = &base {
        for (path, (_, sha, _)) in overrides {
            let Some(rel) = path.strip_prefix("minecraft/") else { continue };
            let local = game.join(rel);
            if local.is_file() && file_sha256(&local).ok().as_deref() == Some(sha.as_str()) {
                skip.insert(path.clone());
            }
        }
    }

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
            .chain(java_recipe(&root))
            .collect();
        zip.start_file("instance.cfg", text).map_err(|e| e.to_string())?;
        zip.write_all(cfg.join("\n").as_bytes()).map_err(|e| e.to_string())?;

        let mut files = 1;
        add_file(&mut zip, &root.join("mmc-pack.json"), "mmc-pack.json")?;
        files += 1;
        if root.join("patches").is_dir() {
            files += add_dir(&mut zip, &root.join("patches"), "patches", &empty_cdn, &mut used_cdn, &skip)?;
        }
        if root.join("jarmods").is_dir() {
            files += add_dir(&mut zip, &root.join("jarmods"), "jarmods", &empty_cdn, &mut used_cdn, &skip)?;
        }
        for extra in ROOT_EXTRA_FILES {
            if root.join(extra).is_file() {
                add_file(&mut zip, &root.join(extra), extra)?;
                files += 1;
            }
        }
        // the whole game folder, except what stays home (GAME_FILES are among the files)
        let mut tops: Vec<std::fs::DirEntry> = std::fs::read_dir(&game).map(|r| r.flatten().collect()).unwrap_or_default();
        tops.sort_by_key(|e| e.file_name());
        for entry in tops {
            let name = entry.file_name().to_string_lossy().to_string();
            if stays_home(&name) {
                continue;
            }
            let rel = format!("minecraft/{}", name);
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                files += add_dir(&mut zip, &entry.path(), &rel, &cdn_map, &mut used_cdn, &skip)?;
            } else if !skip.contains(&rel) {
                add_file(&mut zip, &entry.path(), &rel)?;
                files += 1;
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
        if !used_cdn.is_empty() {
            let cdn_json = serde_json::to_vec(&used_cdn).map_err(|e| e.to_string())?;
            zip.start_file("ika-cdn.json", text).map_err(|e| e.to_string())?;
            zip.write_all(&cdn_json).map_err(|e| e.to_string())?;
        }
        let mut buf_writer = zip.finish().map_err(|e| e.to_string())?;
        buf_writer.flush().map_err(|e| e.to_string())?;
        Ok((files, mods))
    })();
    // with a base, the package says what it builds on and lists every file of the instance
    // (also the ones the base provides), with their hashes
    let result = result.and_then(|counts| {
        if let Some((base_ref, overrides)) = &base {
            let mut manifest = package_manifest(&out)?;
            for path in &skip {
                if let Some((_, sha, _)) = overrides.get(path) {
                    manifest.insert(path.clone(), sha.clone());
                }
            }
            let file = std::fs::OpenOptions::new().read(true).write(true).open(&out).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipWriter::new_append(file).map_err(|e| e.to_string())?;
            let text = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("ika-base.json", text).map_err(|e| e.to_string())?;
            zip.write_all(&serde_json::to_vec(base_ref).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            zip.start_file("ika-manifest.json", text).map_err(|e| e.to_string())?;
            zip.write_all(&serde_json::to_vec(&manifest).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            zip.finish().map_err(|e| e.to_string())?;
        }
        Ok(counts)
    });
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

/// Leftovers of a previous session (the launcher was closed mid-transfer): the packages and
/// partial downloads at the top of share/. Its folders stay: published/ (what this PC published,
/// to tell whether an instance changed since) and bases/ (modpacks kept for two weeks).
#[tauri::command(async)]
pub fn share_cleanup() {
    for entry in std::fs::read_dir(share_dir()).into_iter().flatten().flatten() {
        if entry.file_type().is_ok_and(|t| t.is_file()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
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

/// Where a file of a package goes in the instance at `target` (None: not part of an instance).
fn package_dest(target: &Path, path: &str) -> Option<PathBuf> {
    let mut parts = path.split('/').filter(|c| !c.is_empty() && *c != "." && *c != "..");
    let first = parts.next()?;
    let dest = match first {
        "instance.cfg" => target.join("instance.cfg"),
        "mmc-pack.json" => target.join("mmc-pack.json"),
        "patches" => parts.fold(target.join("patches"), |p, c| p.join(c)),
        "jarmods" => parts.fold(target.join("jarmods"), |p, c| p.join(c)),
        "minecraft" => parts.fold(target.join(".minecraft"), |p, c| p.join(c)),
        extra if ROOT_EXTRA_FILES.contains(&extra) => target.join(extra),
        _ => return None,
    };
    Some(dest)
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
        let rel = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect::<Vec<_>>().join("/");
        if rel == "ika-cdn.json" {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            if let Ok(parsed) = serde_json::from_slice::<Vec<CdnModEntry>>(&buf) {
                cdn_entries = parsed;
            }
            continue;
        }
        let Some(dest) = package_dest(target, &rel) else { continue };
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
                let links: Vec<&String> = std::iter::once(&item.url).chain(item.alts.iter()).collect();
                for (attempt, url) in links.iter().flat_map(|u| [*u, *u]).enumerate() {
                    if ok {
                        break;
                    }
                    if attempt % 2 == 1 {
                        std::thread::sleep(std::time::Duration::from_millis(400));
                    }
                    match agent.get(url).call() {
                        Ok(resp) => {
                            let mut bytes = Vec::with_capacity(item.size as usize);
                            if let Err(e) = resp.into_reader().read_to_end(&mut bytes) {
                                last_err = format!("{}: {}", url, e);
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
                            last_err = format!("{}: {}", url, e);
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
    install_package(&app, &id, &part, &name)
}

/// Turns a received package into a new instance named after `name` (or a free variant of it):
/// unpacks it, downloads the mods it lists from their CDN and prepares Minecraft and Java.
fn install_package(app: &tauri::AppHandle, id: &str, part: &Path, name: &str) -> Result<String, String> {
    use tauri::Emitter;
    let (app, id, part) = (app.clone(), id.to_string(), part.to_path_buf());
    let (final_name, folder) = free_instance_name(name);
    let target = crate::mc::instances_root().join(&folder);
    let result = unpack(&part, &target).and_then(|cdn_entries| {
        if package_base(&part).is_some() {
            let needed: Vec<(String, String)> = package_manifest(&part)?
                .into_iter()
                .filter(|(path, _)| package_dest(&target, path).is_some_and(|d| !d.is_file()))
                .collect();
            fill_from_base(&app, &id, &target, &part, &needed)?;
        }
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

// ---------------------------------------------------------------------------------------------
// Groups: publishing an instance to the people who follow a creator, and installing it
// ---------------------------------------------------------------------------------------------
// What is published is the same package a friend receives (share_pack): the mods Modrinth and
// CurseForge serve go as a list and are downloaded from there, so the service only stores the
// rest and a follower installs with the creator offline. Each version is uploaded whole and, when
// there is a previous one, also as only what changed (plus the hash of every file it is made
// of): a follower who has the previous version downloads just that.

/// Pieces a package is uploaded in (under the service's limit per request). Downloading is
/// one request for the whole package.
const GROUP_CHUNK_BYTES: u64 = 3584 * 1024;
/// sha256 of every file of a package, by its path inside it: what a version is made of.
type Manifest = std::collections::BTreeMap<String, String>;
/// Files of a package that describe it instead of being part of the instance.
const PACKAGE_META: [&str; 3] = ["ika-cdn.json", "ika-manifest.json", "ika-base.json"];
/// What an update reports when the installed copy can't take the "only what changed" package
/// (a file the creator didn't change is missing or not the one published): the frontend then
/// downloads the whole version.
const NEED_FULL: &str = "ika-need-full";
/// Kept in the root of an instance installed from a group: which files came from the creator,
/// the ones an update may replace or remove. Everything else in the instance is the player's.
const GROUP_RECORD: &str = ".ika-group.json";

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct GroupRecord {
    gid: String,
    pid: String,
    ver: String,
    manifest: Manifest,
    /// path inside the package -> sha1, of the mods downloaded from their CDN
    cdn: std::collections::BTreeMap<String, String>,
}

/// The last version this PC published of an instance, to build the next one's changes from.
#[derive(serde::Serialize, serde::Deserialize)]
struct Published {
    ver: String,
    manifest: Manifest,
    /// what the instance looked like then (publish_stamp), to tell whether it changed since
    #[serde(default)]
    stamp: Option<String>,
}

/// A fingerprint of what publishing an instance would package (the same files share_pack takes),
/// cheap enough to work out whenever its group's page opens: small files by their content (so a
/// game rewriting options.txt unchanged doesn't count), big ones by size and modification time.
fn publish_stamp(root: &Path) -> String {
    const SMALL: u64 = 256 * 1024;
    let game = game_dir(root);
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    let walk = |dir: &Path, prefix: &str, files: &mut Vec<(String, PathBuf)>| {
        for entry in walkdir::WalkDir::new(dir).follow_links(false).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let Ok(rel) = entry.path().strip_prefix(dir) else { continue };
            let rel = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            let full = format!("{}/{}", prefix, rel);
            if full.strip_prefix("minecraft/").is_some_and(stays_home) {
                continue;
            }
            files.push((full, entry.path().to_path_buf()));
        }
    };
    files.push(("mmc-pack.json".into(), root.join("mmc-pack.json")));
    for sub in ["patches", "jarmods"] {
        if root.join(sub).is_dir() {
            walk(&root.join(sub), sub, &mut files);
        }
    }
    for extra in ROOT_EXTRA_FILES {
        if root.join(extra).is_file() {
            files.push((extra.to_string(), root.join(extra)));
        }
    }
    for entry in std::fs::read_dir(&game).map(|r| r.flatten().collect::<Vec<_>>()).unwrap_or_default() {
        let name = entry.file_name().to_string_lossy().to_string();
        if stays_home(&name) {
            continue;
        }
        let rel = format!("minecraft/{}", name);
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            walk(&entry.path(), &rel, &mut files);
        } else {
            files.push((rel, entry.path()));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = sha2::Sha256::new();
    // instance.cfg as it is packaged (without what changes on every launch, like play time)
    let cfg: Vec<String> = std::fs::read_to_string(root.join("instance.cfg"))
        .unwrap_or_default()
        .lines()
        .filter(|line| !CFG_DROP.iter().any(|key| line.split_once('=').is_some_and(|(k, _)| k.trim() == *key)))
        .map(str::to_string)
        .collect();
    hasher.update(cfg.join("\n").as_bytes());
    for (rel, path) in files {
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        if meta.len() <= SMALL {
            if let Ok(bytes) = std::fs::read(&path) {
                hasher.update(&bytes);
            }
        } else {
            let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis()).unwrap_or(0);
            hasher.update(format!("{}:{}", meta.len(), mtime).as_bytes());
        }
    }
    hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect()
}

/// Whether the instance changed since this PC last published it in group `gid` as `pid`:
/// Some(true) changed, Some(false) the same, None when this PC doesn't know (never published it
/// from here, or published it before this was recorded).
#[tauri::command(async)]
pub fn group_pack_changed(gid: String, pid: String, instance_path: String) -> Result<Option<bool>, String> {
    let (gid, pid) = (group_id(&gid, 10)?, group_id(&pid, 10)?);
    let root = crate::instance_root_dir(&instance_path)?;
    let recorded = std::fs::read_to_string(published_path(gid, pid))
        .ok()
        .and_then(|text| serde_json::from_str::<Published>(&text).ok())
        .and_then(|p| p.stamp)
        .or_else(|| std::fs::read_to_string(baseline_path(gid, pid)).ok());
    let now = publish_stamp(&root);
    let Some(stamp) = recorded else {
        // published before this PC kept a record (or the record was lost): what the instance
        // is like now counts as published, so only what changes from here on asks to publish
        let path = baseline_path(gid, pid);
        std::fs::create_dir_all(path.parent().unwrap_or(&share_dir())).map_err(|e| e.to_string())?;
        std::fs::write(path, &now).map_err(|e| e.to_string())?;
        return Ok(Some(false));
    };
    Ok(Some(now != stamp))
}

fn group_id(raw: &str, len: usize) -> Result<&str, String> {
    let ok = raw.len() == len && raw.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    ok.then_some(raw).ok_or_else(|| "Identificador de grupo inválido".to_string())
}

fn package_kind(kind: &str) -> Result<&str, String> {
    matches!(kind, "full" | "delta").then_some(kind).ok_or_else(|| "Tipo de paquete inválido".to_string())
}

fn published_path(gid: &str, pid: &str) -> PathBuf {
    share_dir().join("published").join(format!("{}-{}.json", gid, pid))
}

/// The instance's stamp taken as published when there was no record of publishing it.
fn baseline_path(gid: &str, pid: &str) -> PathBuf {
    share_dir().join("published").join(format!("{}-{}.stamp", gid, pid))
}

fn sha256_of(reader: &mut impl Read) -> Result<String, String> {
    let mut hasher = sha2::Sha256::new();
    std::io::copy(reader, &mut hasher).map_err(|e| e.to_string())?;
    Ok(hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect())
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    sha256_of(&mut std::io::BufReader::with_capacity(4 * 1024 * 1024, file))
}

fn open_package(path: &Path) -> Result<zip::ZipArchive<std::io::BufReader<std::fs::File>>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    zip::ZipArchive::new(std::io::BufReader::with_capacity(2 * 1024 * 1024, file)).map_err(|_| "El paquete no es válido".to_string())
}

/// The files a package carries, with their hashes. A package with only the changes says what
/// the whole version is made of in its ika-manifest.json; for a whole one it is what it holds.
fn package_manifest(path: &Path) -> Result<Manifest, String> {
    let mut zip = open_package(path)?;
    if let Ok(mut listed) = zip.by_name("ika-manifest.json") {
        let mut buf = Vec::new();
        listed.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        return serde_json::from_slice(&buf).map_err(|_| "El paquete no es válido".to_string());
    }
    let mut manifest = Manifest::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if entry.is_dir() || PACKAGE_META.contains(&name.as_str()) {
            continue;
        }
        manifest.insert(name, sha256_of(&mut entry)?);
    }
    Ok(manifest)
}

/// Builds `out-{id}d.zip`, the changes of the package `out-{id}.zip` since the version this PC
/// published last of instance `pid` (none: there is nothing to compare with). It holds every
/// file whose hash changed or is new, what describes the instance (loader, settings, the CDN
/// mod list) and the hashes of the whole version.
#[tauri::command(async)]
pub fn group_pack_delta(gid: String, pid: String, id: String) -> Result<Option<serde_json::Value>, String> {
    let (gid, pid) = (group_id(&gid, 10)?, group_id(&pid, 10)?);
    check_id(&id)?;
    let Some(previous) = std::fs::read_to_string(published_path(gid, pid))
        .ok()
        .and_then(|text| serde_json::from_str::<Published>(&text).ok())
    else {
        return Ok(None);
    };
    let delta_id = format!("{}d", id);
    check_id(&delta_id)?;
    let full = outgoing_path(&id);
    let manifest = package_manifest(&full)?;
    let out = outgoing_path(&delta_id);
    let built = (|| -> Result<(), String> {
        let mut source = open_package(&full)?;
        let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(std::fs::File::create(&out).map_err(|e| e.to_string())?));
        for i in 0..source.len() {
            let entry = source.by_index_raw(i).map_err(|e| e.to_string())?;
            let name = entry.name().to_string();
            let describes = PACKAGE_META.contains(&name.as_str()) || name == "mmc-pack.json" || name == "instance.cfg";
            let changed = manifest.get(&name).is_some_and(|sha| previous.manifest.get(&name) != Some(sha));
            if describes || changed {
                zip.raw_copy_file(entry).map_err(|e| e.to_string())?;
            }
        }
        let text = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("ika-manifest.json", text).map_err(|e| e.to_string())?;
        zip.write_all(&serde_json::to_vec(&manifest).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        zip.finish().map_err(|e| e.to_string())?.flush().map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(e) = built {
        let _ = std::fs::remove_file(&out);
        return Err(e);
    }
    let zip_size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    Ok(Some(serde_json::json!({
        "from": previous.ver,
        "id": delta_id,
        "zip_size": zip_size,
        "sha256": file_sha256(&out)?,
    })))
}

/// Remembers what version `ver` of instance `pid` (package `out-{id}.zip`) is made of, once it
/// is published: the next version's changes are taken against it.
#[tauri::command(async)]
pub fn group_pack_remember(gid: String, pid: String, ver: String, id: String, instance_path: Option<String>) -> Result<(), String> {
    let (gid, pid, ver) = (group_id(&gid, 10)?, group_id(&pid, 10)?, group_id(&ver, 8)?);
    check_id(&id)?;
    let stamp = instance_path.and_then(|p| crate::instance_root_dir(&p).ok()).map(|root| publish_stamp(&root));
    let published = Published { ver: ver.to_string(), manifest: package_manifest(&outgoing_path(&id))?, stamp };
    let path = published_path(gid, pid);
    std::fs::create_dir_all(path.parent().unwrap_or(&share_dir())).map_err(|e| e.to_string())?;
    std::fs::write(path, serde_json::to_vec(&published).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn explain_group(err: String, me: &str) -> String {
    let (code, detail) = err.split_once('|').unwrap_or(("", err.as_str()));
    match (code, detail) {
        ("426", _) => UPDATE_REQUIRED.to_string(),
        ("404", "no group") => "Ese grupo ya no existe.".to_string(),
        ("404", "no pack") => "Esa instancia ya no está publicada en el grupo.".to_string(),
        ("403", "bad code") => "El código de invitación no es correcto.".to_string(),
        ("403", "not the owner") => "Solo el creador del grupo puede hacer eso.".to_string(),
        ("403", "not a member") => "Únete al grupo para instalar sus instancias.".to_string(),
        ("409", "too many owned") => "Puedes crear como máximo 3 grupos.".to_string(),
        ("409", "too many groups") => "Ya sigues 30 grupos, el máximo. Deja de seguir alguno para unirte a otro.".to_string(),
        ("409", "too many packs") => "Un grupo puede tener como máximo 50 instancias publicadas.".to_string(),
        ("413", "package too large") => {
            "Esta instancia pesa más de 50 MB sin contar los mods de Modrinth y CurseForge. Quita archivos grandes que no sean mods (mundos, resource packs propios) e inténtalo de nuevo.".to_string()
        }
        ("413", "owner storage full") => "Tus grupos ya ocupan 150 MB, el máximo. Retira alguna instancia publicada para publicar otra.".to_string(),
        ("400", "package incomplete") => "La subida no llegó completa. Vuelve a publicarla.".to_string(),
        _ => explain(err.clone(), me, None),
    }
}

/// A signed call to the groups part of the service: create, find, join, publish... The body
/// and the answer are the service's JSON (skins-service/src/index.js).
#[tauri::command(async)]
pub fn group_call(username: String, method: String, path: String, body: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
    let me = valid_name(&username)?;
    let allowed = ["/groups/create?", "/groups/act?", "/groups/find?", "/groups/code?", "/groups/mine?", "/groups/discover?", "/groups/cover?"];
    if !allowed.iter().any(|p| path.starts_with(p)) || !matches!(method.as_str(), "GET" | "POST") {
        return Err("Llamada de grupos no permitida".to_string());
    }
    let path = format!("{}&name={}", path, me);
    // creating a group asks for a little work (the service checks it)
    let mut body = body;
    if path.starts_with("/groups/create?") {
        if let Some(serde_json::Value::Object(map)) = body.as_mut() {
            map.insert("pow".to_string(), serde_json::Value::String(proof_of_work("group", &me, 20)?.to_string()));
        }
    }
    let bytes = match &body {
        Some(value) if method == "POST" => serde_json::to_vec(value).map_err(|e| e.to_string())?,
        _ => Vec::new(),
    };
    mailbox(&method, &path, &bytes).map_err(|e| explain_group(e, &me))
}

/// Uploads an image for a group or the profile cover (AVIF/WebP/PNG/JPEG, base64). Answers its
/// reference ({ref: "img:{sha256}"}); it is kept once something uses it.
#[tauri::command(async)]
pub fn group_image_upload(username: String, image: String) -> Result<serde_json::Value, String> {
    let me = valid_name(&username)?;
    let bytes = B64.decode(image.trim()).map_err(|_| "La imagen no es válida".to_string())?;
    mailbox("PUT", &format!("/groups/img?name={}", me), &bytes).map_err(|e| explain_group(e, &me))
}

/// Encodes `width` x `height` RGBA pixels (base64) as AVIF at `quality` (0-100): about a third
/// smaller than WebP for the same look. Answers the file in base64.
#[tauri::command(async)]
pub fn encode_avif(rgba: String, width: usize, height: usize, quality: f32) -> Result<String, String> {
    let raw = B64.decode(rgba.trim()).map_err(|e| e.to_string())?;
    if width == 0 || height == 0 || width > 4096 || height > 4096 || raw.len() != width * height * 4 {
        return Err("Tamaño de imagen no válido".to_string());
    }
    let pixels: Vec<ravif::RGBA8> = raw.chunks_exact(4).map(|c| ravif::RGBA8::new(c[0], c[1], c[2], c[3])).collect();
    let encoded = ravif::Encoder::new()
        .with_quality(quality.clamp(1.0, 100.0))
        .with_alpha_quality(quality.clamp(1.0, 100.0))
        .with_speed(6)
        .encode_rgba(ravif::Img::new(&pixels[..], width, height))
        .map_err(|e| e.to_string())?;
    Ok(B64.encode(encoded.avif_file))
}

/// Counts what is read through it, for upload progress.
struct Progress<R, F: FnMut(u64)> {
    inner: R,
    done: u64,
    report: F,
}

impl<R: Read, F: FnMut(u64)> Read for Progress<R, F> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.done += n as u64;
        (self.report)(self.done);
        Ok(n)
    }
}

/// Uploads package `out-{id}.zip` as the `kind` ("full" or "delta") of a version about to be
/// published in group `gid`: straight to R2 through `url` (a link the service signed), or in
/// pieces through the service without one. The group shows it once the frontend announces it
/// ("publish"). Emits `group-upload-progress` {id, sent, total}; answers the number of pieces.
#[tauri::command(async)]
pub fn group_upload(app: tauri::AppHandle, username: String, gid: String, kind: String, id: String, url: Option<String>) -> Result<u32, String> {
    use tauri::Emitter;
    let me = valid_name(&username)?;
    let (gid, kind) = (group_id(&gid, 10)?, package_kind(&kind)?);
    check_id(&id)?;
    let path = outgoing_path(&id);
    let total = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    if let Some(url) = url.filter(|u| u.starts_with("https://")) {
        let mut last_emit = std::time::Instant::now();
        let reader = Progress {
            inner: std::io::BufReader::with_capacity(1024 * 1024, std::fs::File::open(&path).map_err(|e| e.to_string())?),
            done: 0,
            report: |done: u64| {
                if last_emit.elapsed().as_millis() > 150 || done == total {
                    last_emit = std::time::Instant::now();
                    let _ = app.emit("group-upload-progress", serde_json::json!({ "id": id, "sent": done, "total": total }));
                }
            },
        };
        return match http_agent().put(&url).set("content-length", &total.to_string()).send(reader) {
            Ok(_) => Ok(1),
            // the link lasts an hour: the frontend asks for a new one
            Err(ureq::Error::Status(403, _)) => Err("ika-link-expired".to_string()),
            Err(ureq::Error::Status(code, _)) => Err(format!("El almacenamiento respondió {} al subir la instancia.", code)),
            Err(e) => Err(explain_group(format!("0|{}", e), &me)),
        };
    }
    let chunks = total.div_ceil(GROUP_CHUNK_BYTES).max(1) as u32;
    let mut file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let mut sent = 0u64;
    for index in 0..chunks {
        let mut buf = Vec::with_capacity(GROUP_CHUNK_BYTES as usize);
        (&mut file).take(GROUP_CHUNK_BYTES).read_to_end(&mut buf).map_err(|e| e.to_string())?;
        let chunk_path = format!("/groups/chunk?name={}&gid={}&kind={}&i={}", me, gid, kind, index);
        let mut attempt = 0;
        loop {
            match mailbox("PUT", &chunk_path, &buf) {
                Ok(_) => break,
                Err(e) if attempt < 2 && e.starts_with("0|") => {
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(800 * attempt));
                }
                Err(e) => return Err(explain_group(e, &me)),
            }
        }
        sent += buf.len() as u64;
        let _ = app.emit("group-upload-progress", serde_json::json!({ "id": id, "sent": sent, "total": total }));
    }
    Ok(chunks)
}

/// Downloads a published package in one request, writing it to `dest` as it arrives: from R2
/// through `url` (a link the service signed), or through the service without one.
#[allow(clippy::too_many_arguments)]
fn download_package(app: &tauri::AppHandle, id: &str, me: &str, path: &str, url: Option<&str>, total: u64, dest: &Path) -> Result<String, String> {
    use tauri::Emitter;
    let response = if let Some(url) = url.filter(|u| u.starts_with("https://")) {
        match http_agent().get(url).call() {
            // the link lasts an hour: the frontend asks for a new one
            Err(ureq::Error::Status(403, _)) => return Err("ika-link-expired".to_string()),
            Err(ureq::Error::Status(404, _)) => return Err(explain_group("404|no pack".to_string(), me)),
            other => other,
        }
    } else {
        let service = crate::device_key::skins_service_url().ok_or("El servicio de Ika no está configurado")?;
        let key = device_key()?;
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
            .to_string();
        let digest: String = sha2::Sha256::digest(b"").iter().map(|b| format!("{:02x}", b)).collect();
        let signature = key.sign(format!("{}\nGET\n{}\n{}\n{}", crate::device_key::signed_prefix("ika-share-v1"), path, ts, digest).as_bytes());
        http_agent()
            .get(&format!("{}{}", service, path))
            .set("x-ika-pubkey", &B64.encode(key.verifying_key().to_bytes()))
            .set("x-ika-timestamp", &ts)
            .set("x-ika-version", crate::device_key::CLIENT_VERSION)
            .set("x-ika-signature", &B64.encode(signature.to_bytes()))
            .call()
    };
    let response = match response {
        Ok(r) => r,
        Err(ureq::Error::Status(code, r)) => {
            let detail = r
                .into_string()
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
                .unwrap_or_default();
            return Err(explain_group(format!("{}|{}", code, detail), me));
        }
        Err(e) => return Err(explain_group(format!("0|{}", e), me)),
    };
    let mut reader = response.into_reader();
    let mut out = std::io::BufWriter::with_capacity(1024 * 1024, std::fs::File::create(dest).map_err(|e| e.to_string())?);
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let (mut got, mut last_emit) = (0u64, std::time::Instant::now());
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("Se cortó la descarga: {}", e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|e| format!("No se pudo guardar lo descargado: {}", e))?;
        got += n as u64;
        if last_emit.elapsed().as_millis() > 150 {
            last_emit = std::time::Instant::now();
            let _ = app.emit("group-download-progress", serde_json::json!({ "id": id, "got": got, "total": total }));
        }
    }
    out.flush().map_err(|e| e.to_string())?;
    let _ = app.emit("group-download-progress", serde_json::json!({ "id": id, "got": got, "total": total }));
    Ok(hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect())
}

/// Downloads a published package ("full", or "delta": only what changed since the version the
/// copy at `update_path` has; from `url`, the link the service signed for it, when there is one)
/// and installs it: as a new instance, or over `update_path`.
/// Emits `group-download-progress` {id, got, total}, then the `share-transfer-progress` stages
/// of receiving from a friend. An update that needs the whole version fails with NEED_FULL.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn group_install(
    app: tauri::AppHandle,
    username: String,
    gid: String,
    pid: String,
    ver: String,
    kind: String,
    size: u64,
    sha256: String,
    name: String,
    update_path: Option<String>,
    url: Option<String>,
    id: String,
) -> Result<String, String> {
    let me = valid_name(&username)?;
    let (gid, pid, ver, kind) = (group_id(&gid, 10)?, group_id(&pid, 10)?, group_id(&ver, 8)?, package_kind(&kind)?);
    check_id(&id)?;
    std::fs::create_dir_all(share_dir()).map_err(|e| e.to_string())?;
    let part = incoming_path(&id);
    let path = format!("/groups/pack?name={}&gid={}&pid={}&ver={}&kind={}", me, gid, pid, ver, kind);
    let result = download_package(&app, &id, &me, &path, url.as_deref(), size, &part).and_then(|digest| {
        if digest != sha256.to_lowercase() {
            return Err("La instancia llegó incompleta o dañada. Inténtalo de nuevo.".to_string());
        }
        let update = update_path.filter(|p| !p.trim().is_empty());
        match update {
            Some(existing) => crate::instance_root_dir(&existing).and_then(|target| update_installed(&app, &id, &part, &target, gid, pid, ver)),
            None => {
                let record = GroupRecord {
                    gid: gid.to_string(),
                    pid: pid.to_string(),
                    ver: ver.to_string(),
                    manifest: package_manifest(&part)?,
                    cdn: package_cdn(&part)?.into_iter().map(|e| (e.rel, e.sha1)).collect(),
                };
                let installed = install_package(&app, &id, &part, &name)?;
                write_record(Path::new(&installed), &record)?;
                Ok(installed)
            }
        }
    });
    let _ = std::fs::remove_file(&part);
    result
}

fn package_cdn(path: &Path) -> Result<Vec<CdnModEntry>, String> {
    let mut zip = open_package(path)?;
    let Ok(mut listed) = zip.by_name("ika-cdn.json") else { return Ok(Vec::new()) };
    let mut buf = Vec::new();
    listed.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(serde_json::from_slice(&buf).unwrap_or_default())
}

fn write_record(target: &Path, record: &GroupRecord) -> Result<(), String> {
    std::fs::write(target.join(GROUP_RECORD), serde_json::to_vec(record).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Brings an instance installed from a group to the version in `part` (whole, or only what
/// changed). Only the creator's files move: those of the new version are written, those the
/// creator removed are deleted, mods still on the CDN that are already here are kept. What the
/// player added or changed on their side is left alone, and so are their game options, server
/// list and instance settings (memory, Java, name).
#[allow(clippy::too_many_arguments)]
fn update_installed(app: &tauri::AppHandle, id: &str, part: &Path, target: &Path, gid: &str, pid: &str, ver: &str) -> Result<String, String> {
    let old: GroupRecord = std::fs::read(target.join(GROUP_RECORD))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let staging = crate::mc::instances_root().join(format!(".ika-update-{}", id));
    let _ = std::fs::remove_dir_all(&staging);
    let player_files: Vec<String> = GAME_FILES.iter().map(|f| format!("minecraft/{}", f)).collect();
    let result = (|| -> Result<(), String> {
        let manifest = package_manifest(part)?;
        let cdn_entries = unpack(part, &staging)?;
        let has_base = package_base(part).is_some();
        let mut from_base: Vec<(String, String)> = Vec::new();
        // every file of the new version is in the package, unchanged since the installed one, or
        // (built on a modpack) in that modpack
        for (path, sha) in &manifest {
            let (Some(dest), Some(staged)) = (package_dest(target, path), package_dest(&staging, path)) else { continue };
            if path == "instance.cfg" || (player_files.contains(path) && dest.exists()) {
                continue;
            }
            if staged.is_file() {
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                std::fs::copy(&staged, &dest).map_err(|e| format!("No se pudo actualizar {}: {}", dest.display(), e))?;
            } else if old.manifest.get(path) != Some(sha) || !dest.is_file() {
                if !has_base {
                    return Err(NEED_FULL.to_string());
                }
                from_base.push((path.clone(), sha.clone()));
            }
        }
        if !from_base.is_empty() {
            fill_from_base(app, id, target, part, &from_base)?;
        }
        for path in old.manifest.keys().filter(|p| !manifest.contains_key(*p) && !player_files.contains(*p)) {
            if let Some(dest) = package_dest(target, path) {
                let _ = std::fs::remove_file(dest);
            }
        }
        // mods from the CDN: the ones the creator dropped go, the ones already here stay
        let cdn: std::collections::BTreeMap<String, String> = cdn_entries.iter().map(|e| (e.rel.clone(), e.sha1.clone())).collect();
        for rel in old.cdn.keys().filter(|r| !cdn.contains_key(*r)) {
            if let Some(dest) = package_dest(target, rel) {
                let _ = std::fs::remove_file(dest);
            }
        }
        let missing: Vec<CdnModEntry> = cdn_entries
            .into_iter()
            .filter(|e| {
                let here = package_dest(target, &e.rel).and_then(|dest| sha1_of_file(&dest));
                !here.is_some_and(|(sha1, _)| sha1.eq_ignore_ascii_case(&e.sha1))
            })
            .collect();
        if !missing.is_empty() {
            download_cdn_entries(app, id, target, &missing)?;
        }
        write_record(target, &GroupRecord { gid: gid.to_string(), pid: pid.to_string(), ver: ver.to_string(), manifest, cdn })
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result?;
    use tauri::Emitter;
    let _ = app.emit(
        "share-transfer-progress",
        serde_json::json!({
            "id": id,
            "stage": "runtime",
            "message": "Preparando Minecraft, librerías del loader y Java...",
        }),
    );
    crate::mc::preinstall_instance_runtime(Some(app.clone()), target)?;
    Ok(target.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------------------------
// A group's instance built on a modpack: what that modpack already has is taken from it
// ---------------------------------------------------------------------------------------------
// A group package of an instance installed from a CurseForge / Modrinth modpack only carries
// what differs from that modpack's files (its "overrides"), plus the list of every file with
// its hash (ika-manifest.json) and which modpack version it builds on (ika-base.json). A
// follower downloads that modpack from CurseForge / Modrinth and takes the rest from it.

/// The modpack version a group package builds on.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct GroupBase {
    /// "curseforge" or "modrinth"
    platform: String,
    project: String,
    version: String,
    name: String,
}

/// The modpack the instance at `root` was installed from, as recorded in its instance.cfg
/// (finding out which version, once, for instances installed before the version was recorded),
/// with the files it provides: path in a package -> (path in the modpack, sha256, size).
fn group_base(app: &tauri::AppHandle, id: &str, root: &Path) -> Option<(GroupBase, HashMap<String, (String, String, u64)>)> {
    let cfg: HashMap<String, String> = crate::read_instance_cfg(root).into_iter().collect();
    let platform = cfg.get("ManagedPackType").map(|t| t.to_ascii_lowercase()).filter(|t| t == "curseforge" || t == "modrinth")?;
    let project = cfg.get("ManagedPackID").filter(|v| !v.is_empty())?.clone();
    let recorded = cfg.get("ManagedPackVersionID").filter(|v| !v.is_empty() && *v != "0").cloned();
    let (version, name) = match recorded {
        Some(version) => (version, cfg.get("ManagedPackVersionName").cloned().unwrap_or_default()),
        None => detect_base_version(app, id, root, &platform, &project)?,
    };
    let base = GroupBase { platform, project, version, name };
    let archive = base_archive(app, id, &base).map_err(|e| log::warn!("Modpack base no disponible: {e}")).ok()?;
    let overrides = base_overrides(&archive).map_err(|e| log::warn!("Modpack base ilegible: {e}")).ok()?;
    Some((base, overrides))
}

fn cf_api(path: &str) -> Result<serde_json::Value, String> {
    let text = http_agent()
        .get(&format!("https://api.curseforge.com{}", path))
        .set("x-api-key", crate::mc::CF_API_KEY)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn modrinth_api(path: &str) -> Result<serde_json::Value, String> {
    let text = http_agent()
        .get(&format!("https://api.modrinth.com{}", path))
        .set("user-agent", "IkaLauncher/1.5")
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn bases_dir() -> PathBuf {
    share_dir().join("bases")
}

/// The modpack file of `base`, downloaded once into share/bases (modpacks not used for two
/// weeks are deleted from there).
fn base_archive(app: &tauri::AppHandle, id: &str, base: &GroupBase) -> Result<PathBuf, String> {
    use tauri::Emitter;
    let clean = |s: &str| s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect::<String>();
    let path = bases_dir().join(format!("{}-{}-{}.zip", clean(&base.platform), clean(&base.project), clean(&base.version)));
    std::fs::create_dir_all(bases_dir()).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(bases_dir()).into_iter().flatten().flatten() {
        let old = entry.metadata().ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok());
        if old.is_some_and(|age| age.as_secs() > 14 * 24 * 3600) && entry.path() != path {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    if path.is_file() {
        let _ = std::fs::File::options().append(true).open(&path).and_then(|f| f.set_modified(std::time::SystemTime::now()));
        return Ok(path);
    }
    let urls: Vec<String> = if base.platform == "curseforge" {
        let file = cf_api(&format!("/v1/mods/{}/files/{}", base.project, base.version))?;
        let data = file.get("data").ok_or("CurseForge no encontró esa versión del modpack")?;
        let file_id = data.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
        let name = data.get("fileName").and_then(|v| v.as_str()).unwrap_or("");
        let mut urls: Vec<String> = data.get("downloadUrl").and_then(|v| v.as_str()).filter(|u| !u.is_empty()).map(str::to_string).into_iter().collect();
        if file_id > 0 && !name.is_empty() {
            urls.push(crate::mc::cf_cdn_url("edge.forgecdn.net", file_id, name));
            urls.push(crate::mc::cf_cdn_url("mediafilez.forgecdn.net", file_id, name));
        }
        urls
    } else {
        let version = modrinth_api(&format!("/v2/version/{}", base.version))?;
        let files = version.get("files").and_then(|f| f.as_array()).cloned().unwrap_or_default();
        let pick = files
            .iter()
            .find(|f| f.get("primary").and_then(|p| p.as_bool()) == Some(true))
            .or_else(|| files.iter().find(|f| f.get("filename").and_then(|n| n.as_str()).is_some_and(|n| n.ends_with(".mrpack"))));
        pick.and_then(|f| f.get("url").and_then(|u| u.as_str())).map(str::to_string).into_iter().collect()
    };
    let label = if base.name.is_empty() { "el modpack base".to_string() } else { format!("«{}»", base.name) };
    let _ = app.emit("share-transfer-progress", serde_json::json!({ "id": id, "stage": "base", "message": format!("Descargando {} de {}...", label, if base.platform == "curseforge" { "CurseForge" } else { "Modrinth" }) }));
    let part = path.with_extension("part");
    let mut last = String::from("no hay enlace de descarga");
    for url in urls {
        let attempt = (|| -> Result<(), String> {
            let response = http_agent().get(&url).set("user-agent", "IkaLauncher/1.5").call().map_err(|e| e.to_string())?;
            let mut out = std::io::BufWriter::new(std::fs::File::create(&part).map_err(|e| e.to_string())?);
            std::io::copy(&mut response.into_reader(), &mut out).map_err(|e| e.to_string())?;
            out.flush().map_err(|e| e.to_string())
        })();
        match attempt {
            Ok(()) => {
                std::fs::rename(&part, &path).map_err(|e| e.to_string())?;
                return Ok(path);
            }
            Err(e) => last = e,
        }
    }
    let _ = std::fs::remove_file(&part);
    Err(format!("No se pudo descargar {}: {}", label, last))
}

/// The files a modpack puts in the game folder (CurseForge "overrides", Modrinth "overrides"
/// and "client-overrides"), by where they go in a package: "minecraft/config/x" ->
/// (path in the modpack, sha256, size).
fn base_overrides(archive: &Path) -> Result<HashMap<String, (String, String, u64)>, String> {
    let mut zip = open_package(archive)?;
    let prefixes: Vec<String> = if let Ok(mut manifest) = zip.by_name("manifest.json") {
        let mut buf = Vec::new();
        manifest.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        let value: serde_json::Value = serde_json::from_slice(&buf).unwrap_or_default();
        vec![format!("{}/", value.get("overrides").and_then(|o| o.as_str()).unwrap_or("overrides").trim_end_matches('/'))]
    } else {
        vec!["overrides/".to_string(), "client-overrides/".to_string()]
    };
    let mut files = HashMap::new();
    for prefix in &prefixes {
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().to_string();
            if entry.is_dir() || !name.starts_with(prefix.as_str()) {
                continue;
            }
            let rel = &name[prefix.len()..];
            if rel.split('/').any(|c| c == ".." || c.is_empty()) {
                continue;
            }
            let size = entry.size();
            files.insert(format!("minecraft/{}", rel), (name.clone(), sha256_of(&mut entry)?, size));
        }
    }
    Ok(files)
}

/// Which version of the modpack an instance installed before versions were recorded has:
/// among the newest versions for its Minecraft version, the one whose files match it best.
/// Recorded in its instance.cfg so it is only worked out once.
fn detect_base_version(app: &tauri::AppHandle, id: &str, root: &Path, platform: &str, project: &str) -> Option<(String, String)> {
    let mc = std::fs::read_to_string(root.join("mmc-pack.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| {
            v.get("components")?
                .as_array()?
                .iter()
                .find(|c| c.get("uid").and_then(|u| u.as_str()) == Some("net.minecraft"))
                .and_then(|c| c.get("version").and_then(|x| x.as_str()).map(str::to_string))
        })?;
    let candidates: Vec<(String, String)> = if platform == "curseforge" {
        cf_api(&format!("/v1/mods/{}/files?gameVersion={}&pageSize=10", project, mc))
            .ok()?
            .get("data")?
            .as_array()?
            .iter()
            .filter(|f| f.get("fileName").and_then(|n| n.as_str()).is_some_and(|n| n.to_ascii_lowercase().ends_with(".zip")))
            .filter_map(|f| Some((f.get("id")?.as_u64()?.to_string(), f.get("displayName").and_then(|n| n.as_str()).unwrap_or("").to_string())))
            .take(4)
            .collect()
    } else {
        modrinth_api(&format!("/v2/project/{}/version?game_versions=%5B%22{}%22%5D", project, mc))
            .ok()?
            .as_array()?
            .iter()
            .filter_map(|v| Some((v.get("id")?.as_str()?.to_string(), v.get("version_number").and_then(|n| n.as_str()).unwrap_or("").to_string())))
            .take(4)
            .collect()
    };
    let game = game_dir(root);
    let mut hashes: HashMap<String, Option<String>> = HashMap::new();
    let mut best: Option<(u64, String, String, PathBuf)> = None;
    for (version, name) in candidates {
        let base = GroupBase { platform: platform.to_string(), project: project.to_string(), version: version.clone(), name: name.clone() };
        let Ok(archive) = base_archive(app, id, &base) else { continue };
        let Ok(overrides) = base_overrides(&archive) else { continue };
        let (mut matched, mut total) = (0u64, 0u64);
        for (path, (_, sha, size)) in &overrides {
            total += size;
            let local = hashes
                .entry(path.clone())
                .or_insert_with(|| path.strip_prefix("minecraft/").and_then(|rel| file_sha256(&game.join(rel)).ok()));
            if local.as_deref() == Some(sha.as_str()) {
                matched += size;
            }
        }
        let better = best.as_ref().map_or(true, |(score, ..)| matched > *score);
        if better {
            if let Some((.., old)) = best.take() {
                let _ = std::fs::remove_file(old);
            }
            best = Some((matched, version, name, archive));
        } else {
            let _ = std::fs::remove_file(&archive);
        }
        if total > 0 && matched * 100 >= total * 98 {
            break; // as good as it gets
        }
    }
    let (score, version, name, _) = best.filter(|(score, ..)| *score > 0)?;
    log::debug!("Versión del modpack detectada: {} ({} bytes iguales)", name, score);
    let _ = crate::edit_instance_cfg(root, &[("ManagedPackVersionID", version.clone()), ("ManagedPackVersionName", name.clone())], &[]);
    Some((version, name))
}

fn package_base(part: &Path) -> Option<GroupBase> {
    let mut zip = open_package(part).ok()?;
    let mut entry = zip.by_name("ika-base.json").ok()?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

/// Writes into the instance at `target` the files of `needed` (path in a package, sha256) from
/// the modpack the package `part` builds on.
fn fill_from_base(app: &tauri::AppHandle, id: &str, target: &Path, part: &Path, needed: &[(String, String)]) -> Result<(), String> {
    if needed.is_empty() {
        return Ok(());
    }
    let base = package_base(part).ok_or_else(|| NEED_FULL.to_string())?;
    let archive = base_archive(app, id, &base)?;
    let overrides = base_overrides(&archive)?;
    let mut zip = open_package(&archive)?;
    for (path, sha) in needed {
        let Some((entry_name, base_sha, _)) = overrides.get(path).filter(|(_, b, _)| b == sha) else {
            return Err(format!(
                "El modpack «{}» ya no trae «{}» como lo tenía el creador. Pídele que publique la instancia de nuevo.",
                base.name,
                path.trim_start_matches("minecraft/")
            ));
        };
        let _ = base_sha;
        let Some(dest) = package_dest(target, path) else { continue };
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut entry = zip.by_name(entry_name).map_err(|e| e.to_string())?;
        let mut out = std::io::BufWriter::new(std::fs::File::create(&dest).map_err(|e| e.to_string())?);
        std::io::copy(&mut entry, &mut out).map_err(|e| format!("No se pudo extraer {}: {}", dest.display(), e))?;
        out.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The exact Java of an instance, for whoever receives it
// ---------------------------------------------------------------------------------------------

/// When the instance runs on a Java picked by its path (not one chosen in the creation screen,
/// which already travels as IkaJava*), the same runtime described so the receiving launcher
/// can download it: vendor, major version and, for Temurin and Mojang's runtimes, the exact
/// build (Oracle's becomes Temurin of the same major). Unknown vendors travel as nothing (the
/// receiver picks a compatible Java).
fn java_recipe(root: &Path) -> Vec<String> {
    let cfg: HashMap<String, String> = crate::read_instance_cfg(root).into_iter().collect();
    if cfg.get("IkaJavaVendor").is_some_and(|v| !v.is_empty()) {
        return Vec::new();
    }
    let Some(java) = cfg.get("JavaPath").filter(|p| !p.is_empty()) else { return Vec::new() };
    let java = PathBuf::from(java.replace('/', "\\"));
    let Some(home) = java.parent().and_then(|bin| bin.parent()) else { return Vec::new() };
    let Ok(release) = std::fs::read_to_string(home.join("release")) else { return Vec::new() };
    let field = |key: &str| {
        release
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{}=", key)))
            .map(|v| v.trim().trim_matches('"').to_string())
            .unwrap_or_default()
    };
    let version = field("JAVA_VERSION");
    let mut parts = version.split(|c: char| !c.is_ascii_digit());
    let first: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let major = if first == 1 { parts.next().and_then(|p| p.parse().ok()).unwrap_or(0) } else { first };
    if major == 0 {
        return Vec::new();
    }
    let folder = home.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let implementor = field("IMPLEMENTOR");
    let runtime_version = field("JAVA_RUNTIME_VERSION");
    let (vendor, runtime, url) = if folder.starts_with("java-runtime-") || folder == "jre-legacy" {
        ("mojang", folder.clone(), String::new())
    } else if implementor.contains("Adoptium") {
        // the exact Temurin build: jdk-21.0.5+11 / jdk8u432-b06
        let build = runtime_version.trim_end_matches("-LTS");
        let release_name = if major == 8 {
            build.strip_prefix("1.8.0_").map(|b| format!("jdk8u{}", b)).unwrap_or_default()
        } else {
            format!("jdk-{}", build)
        };
        let url = if release_name.is_empty() {
            String::new()
        } else {
            format!("https://api.adoptium.net/v3/binary/version/{}/windows/x64/jre/hotspot/normal/eclipse", release_name.replace('+', "%2B"))
        };
        ("adoptium", format!("ika-adoptium-{}", build), url)
    } else if implementor.contains("Oracle") {
        // Oracle's own builds can't be downloaded unattended (licence): the same major version
        // of OpenJDK, as Temurin, is the closest the receiver can get
        ("adoptium", String::new(), String::new())
    } else if implementor.contains("Azul") {
        ("zulu", String::new(), String::new())
    } else if implementor.contains("IBM") || implementor.contains("OpenJ9") {
        ("semeru", String::new(), String::new())
    } else {
        return Vec::new();
    };
    let mut lines = vec![format!("IkaJavaVendor={}", vendor), format!("IkaJavaMajor={}", major)];
    if !runtime.is_empty() {
        lines.push(format!("IkaJavaRuntime={}", runtime));
    }
    if !url.is_empty() {
        lines.push(format!("IkaJavaUrl={}", url));
    }
    lines
}

// ---------------------------------------------------------------------------------------------
// Comparing instances with a friend
// ---------------------------------------------------------------------------------------------

/// Where the mods of an instance can be downloaded (Modrinth / CurseForge), by their sha1: sent
/// with its mod list when comparing with a friend, so they can download what they are missing.
#[tauri::command(async)]
pub fn compare_links(files: Vec<String>) -> HashMap<String, Vec<String>> {
    use rayon::prelude::*;
    let hashed: Vec<(String, String, u64, PathBuf)> = files
        .into_par_iter()
        .filter_map(|f| {
            let path = PathBuf::from(&f);
            let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
            if !(name.ends_with(".jar") || name.ends_with(".jar.disabled")) || !path.is_file() {
                return None;
            }
            let (sha1, size) = sha1_of_file(&path)?;
            Some((f, sha1, size, path))
        })
        .collect();
    if hashed.is_empty() {
        return HashMap::new();
    }
    cdn_links(&hashed)
}

/// One call about a comparison (see /social/compare in the service).
#[tauri::command(async)]
pub fn social_compare(username: String, body: serde_json::Value) -> Result<serde_json::Value, String> {
    let me = valid_name(&username)?;
    let bytes = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
    mailbox("POST", &format!("/social/compare?name={}", me), &bytes).map_err(|e| match e.split_once('|') {
        Some(("403", "not friends")) => "Solo puedes comparar con tus amigos.".to_string(),
        Some(("404", "compare gone")) => "Esa comparación ya no existe (caduca a las 24 horas).".to_string(),
        Some(("413", _)) => "La instancia tiene demasiados mods para compararla.".to_string(),
        _ => explain(e, &me, None),
    })
}

#[derive(serde::Deserialize)]
pub struct CompareFile {
    filename: String,
    sha1: String,
    links: Vec<String>,
}

/// Downloads into `mods_dir` the mods a friend has and this instance doesn't. Only from Modrinth's
/// and CurseForge's CDNs (whatever links the friend's launcher sent) and only if the file is the
/// one it said (sha1). Emits "compare-download" {id, done, total, name}; answers the names that
/// could not be downloaded.
#[tauri::command(async)]
pub fn compare_download(app: tauri::AppHandle, id: String, mods_dir: String, files: Vec<CompareFile>) -> Result<Vec<String>, String> {
    use tauri::Emitter;
    let dir = PathBuf::from(&mods_dir);
    if !dir.is_dir() {
        return Err("No se encontró la carpeta de mods de la instancia.".to_string());
    }
    const HOSTS: [&str; 4] = ["https://cdn.modrinth.com/", "https://edge.forgecdn.net/", "https://mediafilez.forgecdn.net/", "https://media.forgecdn.net/"];
    let total = files.len();
    let mut failed = Vec::new();
    for (n, file) in files.into_iter().enumerate() {
        let _ = app.emit("compare-download", serde_json::json!({ "id": id, "done": n, "total": total, "name": file.filename }));
        let name = file.filename.trim().to_string();
        let lower = name.to_ascii_lowercase();
        let safe = !name.is_empty() && !name.contains(['/', '\\', ':']) && !name.starts_with('.') && lower.ends_with(".jar");
        let sha1 = file.sha1.to_ascii_lowercase();
        if !safe || sha1.len() != 40 {
            failed.push(name);
            continue;
        }
        let dest = dir.join(&name);
        if dest.exists() {
            continue;
        }
        let mut ok = false;
        for link in file.links.iter().filter(|l| HOSTS.iter().any(|h| l.starts_with(h))) {
            let Ok(resp) = http_agent().get(link).call() else { continue };
            let mut bytes = Vec::new();
            if resp.into_reader().take(256 * 1024 * 1024).read_to_end(&mut bytes).is_err() {
                continue;
            }
            let got: String = sha1::Sha1::digest(&bytes).iter().map(|b| format!("{:02x}", b)).collect();
            if got != sha1 {
                continue;
            }
            let part = dir.join(format!("{}.part", name));
            if std::fs::write(&part, &bytes).is_ok() && std::fs::rename(&part, &dest).is_ok() {
                ok = true;
                break;
            }
            let _ = std::fs::remove_file(&part);
        }
        if !ok {
            failed.push(name);
        }
    }
    let _ = app.emit("compare-download", serde_json::json!({ "id": id, "done": total, "total": total, "name": "" }));
    Ok(failed)
}
