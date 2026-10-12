//! Device key and skin publishing for the Ika skins service (see skins-service/).
//!
//! No accounts: the first device that publishes a player name owns it. The proof of
//! ownership is an Ed25519 key created on this PC and stored encrypted with Windows DPAPI
//! (plus this machine's MachineGuid as extra entropy), so the key file is useless on any
//! other PC. The service only ever sees the public key and signatures.

use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use std::path::PathBuf;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

fn key_path() -> PathBuf {
    crate::mc::ika_root().join("device.key")
}

/// HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid (identifies this Windows install).
fn machine_guid() -> String {
    let mut cmd = std::process::Command::new("reg");
    cmd.args(["query", r"HKLM\SOFTWARE\Microsoft\Cryptography", "/v", "MachineGuid"]);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd.output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .and_then(|out| out.lines().find(|l| l.contains("MachineGuid")).and_then(|l| l.split_whitespace().last().map(str::to_string)))
        .unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn dpapi(data: &[u8], entropy: &[u8], protect: bool) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
    let extra = CRYPT_INTEGER_BLOB { cbData: entropy.len() as u32, pbData: entropy.as_ptr() as *mut u8 };
    let mut output = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
    let ok = unsafe {
        if protect {
            CryptProtectData(&input, std::ptr::null(), &extra, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        } else {
            CryptUnprotectData(&input, std::ptr::null_mut(), &extra, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut output)
        }
    };
    if ok == 0 || output.pbData.is_null() {
        return None;
    }
    let result = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData as _);
    }
    Some(result)
}

#[cfg(not(target_os = "windows"))]
fn dpapi(_data: &[u8], _entropy: &[u8], _protect: bool) -> Option<Vec<u8>> {
    None
}

/// This PC's key. A key file that can't be decrypted here (copied from another PC, or
/// after reinstalling Windows) is set aside and a new key is made: it never grants
/// anything on this machine.
pub fn device_key() -> Result<SigningKey, String> {
    let path = key_path();
    let entropy = machine_guid();
    if let Ok(blob) = std::fs::read(&path) {
        if let Some(secret) = dpapi(&blob, entropy.as_bytes(), false) {
            if let Ok(bytes) = <[u8; 32]>::try_from(secret.as_slice()) {
                return Ok(SigningKey::from_bytes(&bytes));
            }
        }
        let _ = std::fs::rename(&path, path.with_extension("key.invalid"));
    }
    let key = SigningKey::generate(&mut rand::rngs::OsRng);
    let blob = dpapi(&key.to_bytes(), entropy.as_bytes(), true).ok_or("No se pudo proteger la clave del dispositivo")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, blob).map_err(|e| e.to_string())?;
    Ok(key)
}

/// URL of the deployed skins service (skins-service/, see its README). Every launcher
/// publishes there; empty disables publishing.
const DEFAULT_SKINS_SERVICE: &str = "https://ika-skins.ika-skins.workers.dev";

/// Where the skins service lives: IKA_SKINS_SERVICE, ika_root/skins-service.txt, or the
/// built-in URL. None = publishing disabled.
pub fn skins_service_url() -> Option<String> {
    let raw = std::env::var("IKA_SKINS_SERVICE")
        .ok()
        .or_else(|| std::fs::read_to_string(crate::mc::ika_root().join("skins-service.txt")).ok())
        .unwrap_or_else(|| DEFAULT_SKINS_SERVICE.to_string());
    let url = raw.trim().trim_end_matches('/').to_string();
    url.starts_with("http").then_some(url)
}

pub enum PublishOutcome {
    Published { claimed: bool },
    Unchanged,
    NameTaken,
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(data).iter().map(|b| format!("{:02x}", b)).collect()
}

/// Uploads `png` as the public skin of `name` (skipped when that exact skin was already
/// published from here).
pub fn publish_skin(agent: &ureq::Agent, service: &str, name: &str, png: &[u8]) -> Result<PublishOutcome, String> {
    let lname = name.to_lowercase();
    let hash = sha256_hex(png);
    let cache_path = crate::mc::ika_root().join("skin-publish.json");
    let mut cache: serde_json::Map<String, serde_json::Value> = std::fs::read_to_string(&cache_path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let cache_key = format!("{}|{}", service, lname);
    if cache.get(&cache_key).and_then(|v| v.as_str()) == Some(hash.as_str()) {
        return Ok(PublishOutcome::Unchanged);
    }

    let key = device_key()?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
        .to_string();
    let message = format!("ika-skin-v1\nPUT\n{}\n{}\n{}", lname, ts, hash);
    let signature = key.sign(message.as_bytes());
    let url = format!("{}/skins/{}", service, lname);
    let resp = agent
        .put(&url)
        .set("content-type", "image/png")
        .set("x-ika-pubkey", &B64.encode(key.verifying_key().to_bytes()))
        .set("x-ika-timestamp", &ts)
        .set("x-ika-signature", &B64.encode(signature.to_bytes()))
        .send_bytes(png);
    match resp {
        Ok(r) => {
            let body: serde_json::Value = r
                .into_string()
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
            cache.insert(cache_key, serde_json::Value::String(hash));
            let _ = std::fs::write(&cache_path, serde_json::to_string_pretty(&cache).unwrap_or_default());
            Ok(PublishOutcome::Published { claimed: body.get("claimed").and_then(|c| c.as_bool()).unwrap_or(false) })
        }
        Err(ureq::Error::Status(403, _)) => Ok(PublishOutcome::NameTaken),
        Err(ureq::Error::Status(code, r)) => Err(format!("el servicio respondió {}: {}", code, r.into_string().unwrap_or_default())),
        Err(e) => Err(e.to_string()),
    }
}
