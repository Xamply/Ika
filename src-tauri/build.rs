fn main() {
  // Secrets come from .env.local (never committed) or the environment, not from the code:
  // IKA_CF_API_KEY is the CurseForge API key (mc::CF_API_KEY).
  println!("cargo:rerun-if-changed=../.env.local");
  println!("cargo:rerun-if-env-changed=IKA_CF_API_KEY");
  if std::env::var("IKA_CF_API_KEY").map(|k| k.is_empty()).unwrap_or(true) {
    let env = std::fs::read_to_string("../.env.local").unwrap_or_default();
    for line in env.lines() {
      if let Some(value) = line.trim().strip_prefix("IKA_CF_API_KEY=") {
        // written as in a .env file: each $ escaped as \$ (Vite reads it the same way)
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'').replace("\\$", "$");
        println!("cargo:rustc-env=IKA_CF_API_KEY={}", value);
      }
    }
  }
  // The launcher's version (tauri.conf.json): signed calls to the Ika service carry it, and the
  // service turns away versions older than the one it asks for (MIN_CLIENT_VERSION).
  println!("cargo:rerun-if-changed=tauri.conf.json");
  let conf = std::fs::read_to_string("tauri.conf.json").unwrap_or_default();
  let version = conf
    .lines()
    .find_map(|l| l.trim().strip_prefix("\"version\":"))
    .map(|v| v.trim().trim_end_matches(',').trim_matches('"').to_string())
    .unwrap_or_else(|| "0.0.0".to_string());
  println!("cargo:rustc-env=IKA_APP_VERSION={}", version);
  tauri_build::build()
}
