//! Local skin server for offline accounts.
//!
//! Offline players have no Mojang profile, so Minecraft can't download their skin. Like
//! the no-premium launchers (HMCL, PCL2, Pinecone with Ely.by), the game is started with
//! authlib-injector pointing at this server, which speaks the Yggdrasil API that
//! authlib-injector expects (https://yushijinhun.github.io/authlib-injector/en/yggdrasil-server-technical-specification.html):
//! it returns the player's profile with the skin chosen in the launcher, signed with
//! this server's own RSA key (SHA1withRSA), and serves the PNG. It only listens on
//! 127.0.0.1 and lives as long as the launcher.

use base64::Engine;
use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey};
use rsa::signature::{SignatureEncoding, Signer};
use rsa::RsaPrivateKey;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

struct Profile {
    name: String,
    uuid: String,
    png: Vec<u8>,
    hash: String,
    slim: bool,
}

pub struct SkinServer {
    port: u16,
    key: RsaPrivateKey,
    public_pem: String,
    public_der_b64: String,
    profiles: Mutex<HashMap<String, Arc<Profile>>>, // by uuid
}

static SERVER: Mutex<Option<Arc<SkinServer>>> = Mutex::new(None);

/// Requests served (profile lookups, skin downloads), to tell whether a game really
/// fetched the skin.
pub static PROFILE_HITS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub static TEXTURE_HITS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Last requests ("METHOD /path -> status"), for diagnostics.
pub static RECENT_REQUESTS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The server's signing key, kept on disk so it doesn't have to be generated (slow)
/// on every launch.
fn load_or_create_key() -> Result<RsaPrivateKey, String> {
    let path = crate::mc::ika_root().join("yggdrasil-key.der");
    if let Ok(der) = std::fs::read(&path) {
        if let Ok(key) = RsaPrivateKey::from_pkcs8_der(&der) {
            return Ok(key);
        }
    }
    let key = RsaPrivateKey::new(&mut rand::thread_rng(), 2048).map_err(|e| e.to_string())?;
    if let Ok(der) = key.to_pkcs8_der() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, der.as_bytes());
    }
    Ok(key)
}

/// Starts the server the first time it's needed.
fn server() -> Result<Arc<SkinServer>, String> {
    let mut lock = SERVER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = lock.as_ref() {
        return Ok(s.clone());
    }
    let key = load_or_create_key()?;
    let public = key.to_public_key();
    let public_pem = public.to_public_key_pem(rsa::pkcs8::LineEnding::LF).map_err(|e| e.to_string())?;
    let public_der_b64 = B64.encode(public.to_public_key_der().map_err(|e| e.to_string())?.as_bytes());
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("No se pudo iniciar el servidor de skins: {}", e))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let srv = Arc::new(SkinServer { port, key, public_pem, public_der_b64, profiles: Mutex::new(HashMap::new()) });
    let worker = srv.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let s = worker.clone();
            std::thread::spawn(move || {
                let _ = s.handle(stream);
            });
        }
    });
    *lock = Some(srv.clone());
    Ok(srv)
}

/// Registers (or updates) the skin of an offline player and returns the URL to hand to
/// authlib-injector.
pub fn register(name: &str, uuid: &str, png: Vec<u8>, slim: bool) -> Result<String, String> {
    if png.len() < 8 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err("La skin no es una imagen PNG válida".to_string());
    }
    let srv = server()?;
    let hash = {
        use sha2::Digest;
        sha2::Sha256::digest(&png).iter().map(|b| format!("{:02x}", b)).collect::<String>()
    };
    let profile = Profile { name: name.to_string(), uuid: uuid.to_string(), png, hash, slim };
    srv.profiles.lock().unwrap_or_else(|e| e.into_inner()).insert(uuid.to_string(), Arc::new(profile));
    Ok(format!("http://127.0.0.1:{}/", srv.port))
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

impl SkinServer {
    fn sign(&self, data: &str) -> String {
        let signer = rsa::pkcs1v15::SigningKey::<sha1::Sha1>::new(self.key.clone());
        B64.encode(signer.sign(data.as_bytes()).to_vec())
    }

    /// Profile in the Yggdrasil format, with the signed "textures" property.
    fn profile_json(&self, p: &Profile, signed: bool) -> Value {
        let mut skin = json!({ "url": format!("http://127.0.0.1:{}/textures/{}", self.port, p.hash) });
        if p.slim {
            skin["metadata"] = json!({ "model": "slim" });
        }
        let textures = json!({
            "timestamp": now_ms() as u64,
            "profileId": p.uuid,
            "profileName": p.name,
            "textures": { "SKIN": skin },
        });
        let value = B64.encode(serde_json::to_vec(&textures).unwrap_or_default());
        let mut prop = json!({ "name": "textures", "value": value });
        if signed {
            prop["signature"] = json!(self.sign(&value));
        }
        json!({ "id": p.uuid, "name": p.name, "properties": [prop] })
    }

    fn by_uuid(&self, uuid: &str) -> Option<Arc<Profile>> {
        let uuid = uuid.replace('-', "").to_lowercase();
        self.profiles.lock().ok()?.get(&uuid).cloned()
    }

    fn by_name(&self, name: &str) -> Option<Arc<Profile>> {
        self.profiles.lock().ok()?.values().find(|p| p.name.eq_ignore_ascii_case(name)).cloned()
    }

    fn route(&self, method: &str, path: &str, query: &HashMap<String, String>, body: &[u8]) -> (u16, &'static str, Vec<u8>) {
        let json_resp = |v: Value| (200u16, "application/json; charset=utf-8", serde_json::to_vec(&v).unwrap_or_default());
        let no_content = (204u16, "text/plain", Vec::new());
        match (method, path) {
            ("GET", "/") => json_resp(json!({
                "meta": {
                    "serverName": "Ika Launcher",
                    "implementationName": "ika-launcher-skins",
                    "implementationVersion": env!("CARGO_PKG_VERSION"),
                    "feature.non_email_login": true
                },
                // Our own server, plus Ely.by (skins that SkinRestorer & co. hand out may be
                // hosted there; Mojang's minecraft.net / mojang.com are always allowed).
                "skinDomains": ["127.0.0.1", "localhost", "ely.by", ".ely.by"],
                "signaturePublickey": self.public_pem,
            })),
            ("GET", p) if p.starts_with("/sessionserver/session/minecraft/profile/") => {
                let uuid = &p["/sessionserver/session/minecraft/profile/".len()..];
                let signed = query.get("unsigned").map_or(false, |v| v == "false");
                match self.by_uuid(uuid) {
                    Some(prof) => {
                        PROFILE_HITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        json_resp(self.profile_json(&prof, signed))
                    }
                    None => no_content,
                }
            }
            // LAN / joining: the integrated server checks the player it just let in.
            ("GET", "/sessionserver/session/minecraft/hasJoined") => {
                match query.get("username").and_then(|n| self.by_name(n)) {
                    Some(prof) => json_resp(self.profile_json(&prof, true)),
                    None => no_content,
                }
            }
            ("POST", "/sessionserver/session/minecraft/join") => no_content,
            // Name -> profile lookups (also used by the legacy skin polyfill of old versions).
            ("POST", "/api/profiles/minecraft") => {
                let names: Vec<String> = serde_json::from_slice(body).unwrap_or_default();
                let found: Vec<Value> = names
                    .iter()
                    .take(10)
                    .filter_map(|n| self.by_name(n))
                    .map(|p| json!({ "id": p.uuid, "name": p.name }))
                    .collect();
                json_resp(json!(found))
            }
            ("GET", p) if p.starts_with("/api/users/profiles/minecraft/") => {
                match self.by_name(&p["/api/users/profiles/minecraft/".len()..]) {
                    Some(prof) => json_resp(json!({ "id": prof.uuid, "name": prof.name })),
                    None => no_content,
                }
            }
            ("GET", p) if p.starts_with("/textures/") => {
                let hash = &p["/textures/".len()..];
                let png = self.profiles.lock().ok().and_then(|m| m.values().find(|x| x.hash == hash).map(|x| x.png.clone()));
                match png {
                    Some(bytes) => {
                        TEXTURE_HITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        (200, "image/png", bytes)
                    }
                    None => (404, "text/plain", Vec::new()),
                }
            }
            // Newer versions ask api.minecraftservices.com, which authlib-injector sends here.
            ("GET", "/minecraftservices/player/attributes") => json_resp(json!({
                "privileges": {
                    "onlineChat": { "enabled": true },
                    "multiplayerServer": { "enabled": true },
                    "multiplayerRealms": { "enabled": false },
                    "telemetry": { "enabled": false }
                },
                "profanityFilterPreferences": { "profanityFilterOn": false }
            })),
            ("GET", "/minecraftservices/privacy/blocklist") => json_resp(json!({ "blockedProfiles": [] })),
            ("GET", "/minecraftservices/publickeys") => json_resp(json!({
                "profilePropertyKeys": [{ "publicKey": self.public_der_b64 }],
                "playerCertificateKeys": [{ "publicKey": self.public_der_b64 }]
            })),
            _ => (404, "application/json", b"{}".to_vec()),
        }
    }

    fn handle(&self, stream: TcpStream) -> std::io::Result<()> {
        stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut request_line = String::new();
        reader.read_line(&mut request_line)?;
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let target = parts.next().unwrap_or("/").to_string();
        let mut content_length = 0usize;
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
                break;
            }
            if let Some((k, v)) = header.split_once(':') {
                if k.trim().eq_ignore_ascii_case("content-length") {
                    content_length = v.trim().parse().unwrap_or(0).min(64 * 1024);
                }
            }
        }
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body)?;

        let (path, query_str) = target.split_once('?').unwrap_or((target.as_str(), ""));
        let query: HashMap<String, String> = query_str
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let (status, ctype, payload) = self.route(&method, path, &query, &body);
        if let Ok(mut log) = RECENT_REQUESTS.lock() {
            log.push(format!("{} {} -> {}", method, path, status));
            let excess = log.len().saturating_sub(50);
            log.drain(..excess);
        }
        let reason = match status {
            200 => "OK",
            204 => "No Content",
            _ => "Not Found",
        };
        let mut out = stream;
        write!(
            out,
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            status,
            reason,
            ctype,
            payload.len()
        )?;
        out.write_all(&payload)?;
        out.flush()
    }
}
