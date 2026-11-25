use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use windows::Win32::{
    Foundation::{HLOCAL, LocalFree},
    Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData},
};

const VERSION: &str = env!("CARGO_PKG_VERSION"); // Sent as X-Ika-Version and signed (ika-share-v1).
const DEFAULT_SERVICE: &str = "https://ika-skins.ika-skins.workers.dev";
static KEY: OnceLock<SigningKey> = OnceLock::new();

#[derive(Default, Deserialize)]
#[serde(default)]
pub(super) struct State {
    pub friends: Vec<String>,
    pub sent: Vec<String>,
    pub requests: Vec<Request>,
}
#[derive(Deserialize)]
pub(super) struct Request {
    pub from: String,
}

fn key() -> Result<&'static SigningKey, String> {
    if let Some(k) = KEY.get() {
        return Ok(k);
    }
    use std::os::windows::process::CommandExt;
    let guid = std::process::Command::new("reg.exe")
        .creation_flags(0x08000000)
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            s.lines()
                .find(|l| l.contains("MachineGuid"))
                .and_then(|l| l.split_whitespace().last())
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let path = crate::minecraft::ika_root().join("device.key");
    let mut contents = std::fs::read(path)
        .map_err(|_| "Este dispositivo no tiene una identidad de Ika importada.")?;
    if contents.len() > 16 * 1024 {
        return Err("Clave de Ika inválida.".into());
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: contents.len() as u32,
        pbData: contents.as_mut_ptr(),
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: guid.len() as u32,
        pbData: guid.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|_| "Windows no pudo abrir la identidad de Ika guardada en este equipo.")?;
        let seed = (!output.pbData.is_null() && output.cbData == 32)
            .then(|| SigningKey::from_bytes(&*output.pbData.cast::<[u8; 32]>()));
        if !output.pbData.is_null() {
            for i in 0..output.cbData as usize {
                output.pbData.add(i).write_volatile(0);
            }
            let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
        }
        let seed = seed.ok_or("Clave de Ika inválida.")?;
        let _ = KEY.set(seed);
    }
    KEY.get()
        .ok_or_else(|| "No se pudo cargar la clave de Ika.".into())
}
fn name(s: &str) -> Result<String, String> {
    let s = s.trim().trim_start_matches('@');
    crate::minecraft::valid_username(s)
        .then(|| s.to_ascii_lowercase())
        .ok_or_else(|| "Escribe un nombre de usuario válido.".into())
}
fn signed_message(method: &str, path: &str, ts: &str, body: &[u8]) -> String {
    format!(
        "ika-share-v1@{VERSION}\n{method}\n{path}\n{ts}\n{:x}",
        Sha256::digest(body)
    )
}
fn service() -> Result<String, String> {
    let service = std::env::var("IKA_SKINS_SERVICE")
        .ok()
        .or_else(|| {
            std::fs::read_to_string(crate::minecraft::ika_root().join("skins-service.txt")).ok()
        })
        .unwrap_or_else(|| DEFAULT_SERVICE.into());
    let service = service.trim().trim_end_matches('/').to_owned();
    if !service.starts_with("https://") && !service.starts_with("http://") {
        return Err("El servicio de Ika no está configurado.".into());
    }
    Ok(service)
}
pub(super) fn public_head(name: &str) -> Option<image::RgbaImage> {
    let key = self::name(name).ok()?;
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(4))
        .redirects(2)
        .build();
    let first = service().ok().map(|s| format!("{s}/skins/{key}.png"));
    let urls = first
        .into_iter()
        .chain([format!("https://minotar.net/skin/{name}")]);
    for url in urls {
        let result = agent.get(&url).call().ok().and_then(|r| {
            let mut bytes = Vec::new();
            r.into_reader()
                .take(2 * 1024 * 1024)
                .read_to_end(&mut bytes)
                .ok()?;
            super::storage::rgba(&bytes, 512).filter(|t| t.width() >= 64 && t.height() >= 32)
        });
        if let Some(skin) = result {
            return Some(super::model::head(&skin));
        }
    }
    super::storage::rgba(include_bytes!("steve.png"), 512).map(|t| super::model::head(&t))
}
fn request(method: &str, path: &str, body: &[u8]) -> Result<State, String> {
    let key = key()?;
    let service = service()?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .to_string();
    let sig = key.sign(signed_message(method, path, &ts, body).as_bytes());
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .redirects(0)
        .build();
    let req = agent
        .request(method, &format!("{service}{path}"))
        .set(
            "x-ika-pubkey",
            &STANDARD.encode(key.verifying_key().to_bytes()),
        )
        .set("x-ika-timestamp", &ts)
        .set("x-ika-version", VERSION)
        .set("x-ika-signature", &STANDARD.encode(sig.to_bytes()));
    let response = if method == "POST" {
        req.set("content-type", "application/json").send_bytes(body)
    } else {
        req.call()
    };
    let response = response.map_err(|e| match e {
        ureq::Error::Status(403, _) => {
            "La cuenta no está verificada para la identidad de Ika de este equipo.".into()
        }
        ureq::Error::Status(404, _) => "El usuario o la solicitud no existe en Ika.".into(),
        ureq::Error::Status(409, _) => {
            "Ika no pudo completar esa solicitud. Actualiza la lista e inténtalo de nuevo.".into()
        }
        ureq::Error::Status(code, _) => format!("El servicio de Ika respondió con error {code}."),
        _ => "No se pudo conectar con el servicio de Ika.".into(),
    })?;
    serde_json::from_reader(response.into_reader().take(4 * 1024 * 1024))
        .map_err(|_| "Respuesta inválida del servicio de Ika.".into())
}
pub(super) fn sync(username: &str) -> Result<State, String> {
    request(
        "GET",
        &format!("/social/sync?name={}", name(username)?),
        &[],
    )
}
pub(super) fn friend(username: &str, action: &str, who: &str) -> Result<State, String> {
    let me = name(username)?;
    let who = name(who)?;
    if me == who {
        return Err("No puedes añadirte a ti mismo.".into());
    }
    if !matches!(
        action,
        "request" | "accept" | "decline" | "cancel" | "remove"
    ) {
        return Err("Acción inválida.".into());
    }
    let body = serde_json::json!({"action":action,"who":who}).to_string();
    request(
        "POST",
        &format!("/social/friend?name={me}"),
        body.as_bytes(),
    )?;
    sync(username)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn webview_signature_contract() {
        assert_eq!(
            signed_message("GET", "/social/sync?name=player", "42", &[]),
            format!("ika-share-v1@{VERSION}\nGET\n/social/sync?name=player\n42\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
        assert_eq!(name(" @Some_User ").unwrap(), "some_user");
        assert!(name("x&admin=1").is_err());
    }
}
