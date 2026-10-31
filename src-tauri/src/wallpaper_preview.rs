use std::{hash::{Hash, Hasher}, path::PathBuf, sync::Mutex};
use tauri::Manager;

// Serial generation: hovering must never start several full-size decoders at once.
static PREVIEW_JOB: Mutex<()> = Mutex::new(());

#[tauri::command]
pub async fn get_wallpaper_button_preview(app: tauri::AppHandle, video_path: String) -> Result<Option<String>, String> {
    let cache = app.path().app_cache_dir().map_err(|e| e.to_string())?.join("button-previews-v1");
    tauri::async_runtime::spawn_blocking(move || make_preview(cache, video_path))
        .await.map_err(|e| e.to_string())?
}

fn make_preview(cache: PathBuf, video_path: String) -> Result<Option<String>, String> {
    let source = std::fs::canonicalize(&video_path).map_err(|e| e.to_string())?;
    let extension = source.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    if !matches!(extension.as_str(), "mp4" | "webm") { return Err("Unsupported preview video".into()); }
    let metadata = source.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() { return Err("Preview source must be a file".into()); }
    let mut key = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut key);
    metadata.len().hash(&mut key);
    metadata.modified().map_err(|e| e.to_string())?.hash(&mut key);
    let target = cache.join(format!("{:016x}.mp4", key.finish()));
    let _job = PREVIEW_JOB.lock().map_err(|e| e.to_string())?;
    if target.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return Ok(Some(target.to_string_lossy().into_owned()));
    }
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let temp = target.with_extension("tmp.mp4");
    // Optional tool. Without it, frontend keeps its still image; never falls back to 4K playback.
    let mut command = std::process::Command::new("ffmpeg");
    command.args(["-nostdin", "-v", "error", "-y", "-threads", "1", "-filter_threads", "1", "-i"])
        .arg(&source)
        .args(["-t", "3", "-an", "-sn", "-vf", "fps=15,scale=320:180:force_original_aspect_ratio=decrease,scale=trunc(iw/2)*2:trunc(ih/2)*2", "-c:v", "libx264", "-preset", "ultrafast", "-crf", "25", "-pix_fmt", "yuv420p", "-threads", "1", "-movflags", "+faststart"])
        .arg(&temp)
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000 | 0x40); // no window, idle priority
    }
    let result = command.spawn().and_then(|mut child| {
        let started = std::time::Instant::now();
        loop {
            if let Some(status) = child.try_wait()? { break Ok(status); }
            if started.elapsed() > std::time::Duration::from_secs(20) {
                let _ = child.kill();
                let _ = child.wait();
                break Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Preview encoding timed out"));
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });
    match result {
        Ok(status) if status.success() && temp.metadata().map(|m| m.len() > 0).unwrap_or(false) => {
            std::fs::rename(&temp, &target).map_err(|e| e.to_string())?;
            Ok(Some(target.to_string_lossy().into_owned()))
        }
        result => {
            let _ = std::fs::remove_file(&temp);
            if let Err(e) = result { if e.kind() != std::io::ErrorKind::NotFound { return Err(e.to_string()); } }
            Ok(None)
        }
    }
}
