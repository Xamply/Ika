//! Data sources shared with the WebView2 instance selector.
use super::{engine, java};
use serde_json::Value;
use std::{io::Read, path::Path, sync::LazyLock, time::Duration};

pub(super) const PRISM: &str = "https://meta.prismlauncher.org/v1";
pub(super) const MODRINTH: &str = "https://api.modrinth.com/v2";

pub(super) fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(concat!("IkaLauncher/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(9))
        .build()
}

pub(super) fn json(response: Result<ureq::Response, ureq::Error>) -> Result<Value, String> {
    let body = response.map_err(|e| match e {
        ureq::Error::Status(status, _) => format!("HTTP {status}"),
        ureq::Error::Transport(_) => "No se pudo conectar con el catálogo.".into(),
    })?;
    const LIMIT: u64 = 24 * 1024 * 1024;
    let mut bytes = Vec::new();
    body.into_reader()
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("La respuesta del catálogo es demasiado grande.".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "El catálogo devolvió datos no válidos.".into())
}

pub(super) fn items(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}
pub(super) fn s<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

#[derive(Clone, Debug)]
pub struct LoaderVersion {
    pub version: String,
    pub recommended: bool,
    pub kind: &'static str,
}

fn parse_loaders(data: &Value, loader: &str, mc: &str) -> Vec<LoaderVersion> {
    let prism = matches!(loader, "Forge" | "NeoForge");
    let mut list = items(if prism { &data["versions"] } else { data })
        .filter(|v| {
            !prism
                || items(&v["requires"]).any(|r| r["uid"] == "net.minecraft" && r["equals"] == mc)
        })
        .filter_map(|v| {
            let version = s(if prism { v } else { &v["loader"] }, "version");
            if !super::instances::version_token(version) {
                return None;
            }
            let recommended = v["recommended"] == true || v["loader"]["stable"] == true;
            let beta = v["type"] == "beta"
                || ["beta", "alpha", "rc."]
                    .iter()
                    .any(|mark| version.contains(mark));
            Some(LoaderVersion {
                version: version.into(),
                recommended,
                kind: if beta { "Beta" } else { "Estable" },
            })
        })
        .take(40)
        .collect::<Vec<_>>();
    if !list.iter().any(|v| v.recommended)
        && let Some(v) = list.first_mut()
    {
        v.recommended = true;
    }
    list
}

pub fn loader_versions(
    client: &ureq::Agent,
    loader: &str,
    mc: &str,
) -> Result<Vec<LoaderVersion>, String> {
    if !super::instances::version_token(mc) {
        return Err("Versión de Minecraft no válida.".into());
    }
    let uid = engine::loader_uid_and_name(loader)
        .ok_or("Loader no válido.")?
        .0;
    let url = match loader {
        "Fabric" => format!("https://meta.fabricmc.net/v2/versions/loader/{mc}"),
        "Quilt" => format!("https://meta.quiltmc.org/v3/versions/loader/{mc}"),
        _ => format!("{PRISM}/{uid}/index.json"),
    };
    let result = match client.get(&url).call() {
        Err(ureq::Error::Status(404, _)) if matches!(loader, "Fabric" | "Quilt") => Ok(Vec::new()),
        response => json(response).map(|v| parse_loaders(&v, loader, mc)),
    };
    if loader == "Forge" && !result.as_ref().is_ok_and(|v| !v.is_empty())
        && let Ok(promos) = json(client.get("https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json").call())
    {
        let mut versions = Vec::new();
        for (suffix, kind) in [("recommended", "Recomendado"), ("latest", "Último")] {
            let version = s(&promos["promos"], &format!("{mc}-{suffix}"));
            if super::instances::version_token(version) && !versions.iter().any(|v: &LoaderVersion| v.version == version) {
                versions.push(LoaderVersion { version: version.into(), recommended: versions.is_empty(), kind });
            }
        }
        if !versions.is_empty() {
            return Ok(versions);
        }
    }
    result
}

pub struct JavaVendor {
    pub id: &'static str,
    pub label: &'static str,
    pub sub: &'static str,
    uid: &'static str,
    pub defaults: &'static [u32],
}

pub const JAVA_VENDORS: [JavaVendor; 4] = [
    JavaVendor {
        id: "mojang",
        label: "Mojang",
        sub: "Oficial Minecraft",
        uid: "net.minecraft.java",
        defaults: &[25, 21, 17, 16, 8],
    },
    JavaVendor {
        id: "adoptium",
        label: "Adoptium",
        sub: "Eclipse Temurin",
        uid: "net.adoptium.java",
        defaults: &[27, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17, 11, 8],
    },
    JavaVendor {
        id: "zulu",
        label: "Azul Zulu",
        sub: "Alto rendimiento",
        uid: "com.azul.java",
        defaults: &[
            27, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17, 16, 15, 14, 13, 12, 11, 9, 8,
        ],
    },
    JavaVendor {
        id: "semeru",
        label: "IBM Semeru Open",
        sub: "Motor OpenJ9",
        uid: "com.ibm.java",
        defaults: &[27, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17, 16, 11, 8],
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JavaRuntime {
    pub vendor: String,
    pub major: u32,
    pub name: String,
    pub url: String,
}

impl JavaRuntime {
    pub fn is_installed_at(&self, path: &str) -> bool {
        let folder = java::java_runtime_folder(&self.vendor, self.major, Some(&self.name));
        Path::new(path).ancestors().skip(2).take(2).any(|p| {
            p.file_name().is_some_and(|n| {
                n.eq_ignore_ascii_case(&folder) || n.eq_ignore_ascii_case(&self.name)
            })
        })
    }
}

#[derive(Clone)]
pub struct JavaBuild {
    pub runtime: JavaRuntime,
    pub version: String,
    pub date: String,
    pub package: String,
}

pub fn java_majors(vendor: usize) -> Result<Vec<u32>, String> {
    let url = format!("{PRISM}/{}/index.json", JAVA_VENDORS[vendor].uid);
    let data = json(agent().get(&url).call())?;
    let mut majors = items(&data["versions"])
        .filter_map(|v| s(v, "version").strip_prefix("java")?.parse::<u32>().ok())
        .filter(|v| *v > 0)
        .collect::<Vec<_>>();
    majors.sort_unstable_by(|a, b| b.cmp(a));
    majors.dedup();
    Ok(majors)
}

fn parse_java_builds(data: &Value, vendor: &JavaVendor, major: u32) -> Vec<JavaBuild> {
    let os = if cfg!(target_arch = "aarch64") {
        "windows-arm64"
    } else {
        "windows-x64"
    };
    let mut builds = items(&data["runtimes"])
        .filter(|r| r["runtimeOS"] == os)
        .filter_map(|r| {
            let name = s(r, "name");
            let url = s(r, "url");
            if name.is_empty() || name.chars().any(char::is_control) || !https(url) {
                return None;
            }
            let num = |key| r["version"][key].as_u64().unwrap_or(0);
            let build = num("build");
            let mut parts = vec![
                r["version"]["major"].as_u64().unwrap_or(major.into()),
                num("minor"),
                num("security"),
                build,
            ];
            while parts.len() > 1 && parts.last() == Some(&0) {
                parts.pop();
            }
            let version = parts
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(".");
            let order = (parts[0], num("minor"), num("security"), build == 0, build);
            Some((
                order,
                JavaBuild {
                    runtime: JavaRuntime {
                        vendor: vendor.id.into(),
                        major,
                        name: name.into(),
                        url: url.into(),
                    },
                    version,
                    date: s(r, "releaseTime").into(),
                    package: s(r, "packageType").into(),
                },
            ))
        })
        .collect::<Vec<_>>();
    builds.sort_by(|(a, l), (b, r)| {
        b.cmp(a)
            .then(r.date.cmp(&l.date))
            .then(r.runtime.name.cmp(&l.runtime.name))
    });
    builds.into_iter().map(|(_, b)| b).collect()
}

pub fn java_builds(vendor: usize, major: u32) -> Result<Vec<JavaBuild>, String> {
    let vendor = &JAVA_VENDORS[vendor];
    let url = format!("{PRISM}/{}/java{major}.json", vendor.uid);
    let data = json(agent().get(&url).call())?;
    Ok(parse_java_builds(&data, vendor, major))
}

pub(super) fn https(url: &str) -> bool {
    url.starts_with("https://") && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

include!(concat!(env!("OUT_DIR"), "/sealed.rs"));

/// Unmasks a value sealed by build.rs. black_box keeps the optimizer from
/// folding the result back into plain text inside the binary.
fn unseal((data, pad): (&[u8], &[u8])) -> String {
    let pad = std::hint::black_box(pad);
    String::from_utf8(data.iter().zip(pad).map(|(b, p)| b ^ p).collect()).unwrap_or_default()
}

/// CurseForge API base, sealed at build time.
pub(super) static CURSEFORGE: LazyLock<String> = LazyLock::new(|| unseal(BASE));

/// CurseForge API key, sealed at build time from IKA_SEED (see build.rs).
pub(super) fn cf_key() -> Result<String, String> {
    let key = unseal(SEED);
    if key.is_empty() {
        return Err("Esta compilación no tiene acceso a CurseForge.".into());
    }
    Ok(key)
}

pub(super) fn cf_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(concat!("IkaLauncher/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(9))
        .redirects(0)
        .build()
}

pub(super) fn cf_json(client: &ureq::Agent, key: &str, endpoint: &str) -> Result<Value, String> {
    json(
        client
            .get(&format!("{}/{endpoint}", *CURSEFORGE))
            .set("x-api-key", key)
            .call(),
    )
}

pub fn loader_agent() -> ureq::Agent {
    agent()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    #[ignore = "calls the live CurseForge API with the sealed key"]
    fn sealed_key_reaches_curseforge() {
        let game = cf_json(&cf_agent(), &cf_key().unwrap(), "games/432").unwrap();
        assert_eq!(game["data"]["id"], 432);
    }

    #[test]
    fn forge_and_neoforge_match_the_exact_minecraft_requirement() {
        let data = json!({"versions": [
            {"version":"21.11.0-beta","requires":[{"uid":"net.minecraft","equals":"1.21.11"}]},
            {"version":"21.1.1","requires":[{"uid":"net.minecraft","equals":"1.21.1"}]},
            {"version":"21.1.0","recommended":true,"requires":[{"uid":"net.minecraft","equals":"1.21.1"}]}
        ]});
        let got = parse_loaders(&data, "NeoForge", "1.21.1");
        assert_eq!(
            got.iter().map(|g| g.version.as_str()).collect::<Vec<_>>(),
            ["21.1.1", "21.1.0"]
        );
        assert!(got[1].recommended);
        assert!(!got[0].recommended);
        assert!(parse_loaders(&data, "NeoForge", "1.20.1").is_empty());
    }

    #[test]
    fn prism_java_builds_use_exact_names_and_filter_the_platform() {
        let os = if cfg!(target_arch = "aarch64") {
            "windows-arm64"
        } else {
            "windows-x64"
        };
        let data = json!({"runtimes": [
            {"name":"linux","runtimeOS":"linux-x64","version":{"major":21,"security":14},"url":"https://example.test/1"},
            {"name":"temurin-with-build","runtimeOS":os,"version":{"major":21,"security":13,"build":1},"url":"https://example.test/2"},
            {"name":"temurin-no-build","runtimeOS":os,"version":{"major":21,"security":13},"url":"https://example.test/3"}
        ]});
        let got = parse_java_builds(&data, &JAVA_VENDORS[1], 21);
        assert_eq!(got[0].runtime.name, "temurin-no-build");
        assert_eq!(got[1].version, "21.0.13.1");
        assert_eq!(got.len(), 2);
        assert!(
            got[0]
                .runtime
                .is_installed_at("C:/Ika/java/temurin-no-build/bin/javaw.exe")
        );
        assert!(
            !got[0]
                .runtime
                .is_installed_at("C:/Ika/java/another-jre/bin/javaw.exe")
        );
    }
}
