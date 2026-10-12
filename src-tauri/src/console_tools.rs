//! Backend of the console section: live stats of a running game, and the instance's crash
//! reports and log files (including old gzip-compressed logs).

use serde::Serialize;
use std::path::{Path, PathBuf};

fn instance_dir(instance_path: &str) -> Option<PathBuf> {
    Path::new(instance_path)
        .ancestors()
        .take(4)
        .find(|p| p.join("mmc-pack.json").exists() || p.join("instance.cfg").exists() || p.join("instance.json").exists())
        .map(|p| p.to_path_buf())
}

fn game_dir(inst: &Path) -> PathBuf {
    if inst.join(".minecraft").is_dir() {
        inst.join(".minecraft")
    } else if inst.join("minecraft").is_dir() {
        inst.join("minecraft")
    } else if inst.join("instance.json").is_file() || inst.join("mods").is_dir() {
        inst.to_path_buf()
    } else {
        inst.join("minecraft")
    }
}

#[derive(Serialize)]
pub struct GameStats {
    running: bool,
    pid: u32,
    memory_mb: f64,
    /// CPU use of the game, % of the whole machine (like Task Manager).
    cpu_percent: f64,
    uptime_secs: u64,
}

/// Key of the instance in mc::RUNNING_GAMES.
fn running_key(instance_path: &str) -> String {
    instance_dir(instance_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| instance_path.to_string())
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

#[cfg(target_os = "windows")]
fn kill_process(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle.is_null() {
            return false;
        }
        let ok = TerminateProcess(handle, 1) != 0;
        CloseHandle(handle);
        ok
    }
}

#[cfg(not(target_os = "windows"))]
fn kill_process(_pid: u32) -> bool {
    false
}

/// Force-closes the game started from this instance (it hangs, or the user wants out now). Its
/// exit is then reported as a close, not as a crash.
#[tauri::command(async)]
pub fn stop_game(instance_path: String) -> Result<(), String> {
    let key = running_key(&instance_path);
    let pid = crate::mc::RUNNING_GAMES
        .lock()
        .ok()
        .and_then(|m| m.as_ref().and_then(|m| m.get(&key).map(|entry| entry.0)))
        .ok_or("El juego no está en ejecución")?;
    let stopped = &crate::mc::STOPPED_GAMES;
    stopped.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(Default::default).insert(key.clone());
    if !kill_process(pid) {
        if let Some(set) = stopped.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            set.remove(&key);
        }
        return Err("No se pudo cerrar el juego".to_string());
    }
    Ok(())
}

/// Last CPU sample per pid: (process CPU time in 100ns units, when).
static CPU_SAMPLES: std::sync::Mutex<Option<std::collections::HashMap<u32, (u64, std::time::Instant)>>> =
    std::sync::Mutex::new(None);

#[cfg(target_os = "windows")]
fn process_usage(pid: u32) -> Option<(f64, u64)> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut mem: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        mem.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let mem_ok = K32GetProcessMemoryInfo(handle, &mut mem, mem.cb) != 0;
        let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        let times_ok = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0;
        CloseHandle(handle);
        if !mem_ok || !times_ok {
            return None;
        }
        let ft = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
        Some((mem.WorkingSetSize as f64 / 1_048_576.0, ft(kernel) + ft(user)))
    }
}

#[cfg(not(target_os = "windows"))]
fn process_usage(_pid: u32) -> Option<(f64, u64)> {
    None
}

#[tauri::command(async)]
pub fn game_stats(instance_path: String) -> GameStats {
    let key = running_key(&instance_path);
    let entry = crate::mc::RUNNING_GAMES
        .lock()
        .ok()
        .and_then(|m| m.as_ref().and_then(|m| m.get(&key).copied()));
    let Some((pid, started)) = entry else {
        return GameStats { running: false, pid: 0, memory_mb: 0.0, cpu_percent: 0.0, uptime_secs: 0 };
    };
    let (memory_mb, cpu_time) = process_usage(pid).unwrap_or((0.0, 0));
    let now = std::time::Instant::now();
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
    let mut cpu_percent = 0.0;
    if let Ok(mut lock) = CPU_SAMPLES.lock() {
        let samples = lock.get_or_insert_with(Default::default);
        if let Some((prev_cpu, prev_at)) = samples.get(&pid) {
            let wall = now.duration_since(*prev_at).as_secs_f64();
            if wall > 0.0 && cpu_time >= *prev_cpu {
                cpu_percent = ((cpu_time - prev_cpu) as f64 / 1e7) / wall / cores * 100.0;
            }
        }
        samples.insert(pid, (cpu_time, now));
    }
    GameStats {
        running: true,
        pid,
        memory_mb: (memory_mb * 10.0).round() / 10.0,
        cpu_percent: (cpu_percent * 10.0).round() / 10.0,
        uptime_secs: started.elapsed().as_secs(),
    }
}

#[derive(Serialize)]
pub struct LogFile {
    name: String,
    path: String,
    kind: String, // "crash" | "log" | "launcher"
    modified: u64, // unix seconds
    size: u64,
    /// Crash reports: "Description: ..." and the exception line.
    summary: Option<String>,
}

fn modified_secs(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn crash_summary(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut desc = None;
    let mut exception = None;
    let mut lines = text.lines();
    while let Some(l) = lines.next() {
        if let Some(d) = l.strip_prefix("Description:") {
            desc = Some(d.trim().to_string());
            exception = lines.find(|x| !x.trim().is_empty()).map(|x| x.trim().to_string());
            break;
        }
    }
    match (desc, exception) {
        (Some(d), Some(e)) => Some(format!("{}\n{}", d, e)),
        (Some(d), None) => Some(d),
        _ => None,
    }
}

/// Crash reports and log files of an instance, newest first.
#[tauri::command(async)]
pub fn list_instance_logs(instance_path: String) -> Result<Vec<LogFile>, String> {
    let inst = instance_dir(&instance_path).ok_or("No se encontró la instancia")?;
    let gdir = game_dir(&inst);
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(gdir.join("crash-reports")) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map_or(false, |x| x == "txt") {
                if let Ok(meta) = e.metadata() {
                    out.push(LogFile {
                        name: e.file_name().to_string_lossy().to_string(),
                        path: p.to_string_lossy().to_string(),
                        kind: "crash".to_string(),
                        modified: modified_secs(&meta),
                        size: meta.len(),
                        summary: crash_summary(&p),
                    });
                }
            }
        }
    }
    if let Ok(rd) = std::fs::read_dir(gdir.join("logs")) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            if !(lower.ends_with(".log") || lower.ends_with(".log.gz")) {
                continue;
            }
            if let Ok(meta) = e.metadata() {
                out.push(LogFile {
                    kind: if lower.starts_with("ika-launcher") { "launcher" } else { "log" }.to_string(),
                    path: e.path().to_string_lossy().to_string(),
                    name,
                    modified: modified_secs(&meta),
                    size: meta.len(),
                    summary: None,
                });
            }
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(out)
}

/// Text of a crash report or log file of the instance (gzip logs decompressed). Only the
/// last 4 MB of huge files are returned, or only the last `tail_kb` KB when given.
#[tauri::command(async)]
pub fn read_instance_log(instance_path: String, file_path: String, tail_kb: Option<usize>) -> Result<String, String> {
    let max_bytes: usize = tail_kb.map(|k| k.max(1) * 1024).unwrap_or(4 * 1024 * 1024);
    let inst = instance_dir(&instance_path).ok_or("No se encontró la instancia")?;
    let file = PathBuf::from(&file_path);
    // Only files inside this instance can be read.
    let inst_canon = std::fs::canonicalize(&inst).map_err(|e| e.to_string())?;
    let file_canon = std::fs::canonicalize(&file).map_err(|e| e.to_string())?;
    if !file_canon.starts_with(&inst_canon) {
        return Err("Ese archivo no pertenece a la instancia".to_string());
    }
    let raw = std::fs::read(&file_canon).map_err(|e| e.to_string())?;
    let bytes = if file_path.to_lowercase().ends_with(".gz") {
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&raw[..]), &mut out).map_err(|e| format!("No se pudo descomprimir: {}", e))?;
        out
    } else {
        raw
    };
    let mut start = bytes.len().saturating_sub(max_bytes);
    if start > 0 {
        // Start at a line boundary so the first line is not cut in half.
        if let Some(nl) = bytes[start..].iter().position(|&b| b == b'\n') {
            start += nl + 1;
        }
    }
    let mut text = String::from_utf8_lossy(&bytes[start..]).to_string();
    if start > 0 && tail_kb.is_none() {
        text.insert_str(0, "[... archivo muy grande: se muestran los últimos 4 MB ...]\n");
    }
    Ok(text)
}
