use std::{env, path::PathBuf, process::ExitCode};

use ika_wgpu_wallpaper_bridge::video::{Mp4Player, Mp4PlayerConfig, validate_mp4_path};

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let Some(video) = args.next().map(PathBuf::from) else {
        eprintln!("Uso: mp4_player <archivo.mp4> [video-mpv.exe] [pid]");
        return ExitCode::from(2);
    };
    let backend = args.next().map(PathBuf::from).unwrap_or_else(|| {
        env::current_exe()
            .unwrap_or_default()
            .with_file_name("video-mpv.exe")
    });
    let parent_pid = args
        .next()
        .and_then(|pid| pid.to_str()?.parse().ok())
        .unwrap_or_else(std::process::id);
    if let Err(error) = validate_mp4_path(&video) {
        eprintln!("{error}");
        return ExitCode::from(2);
    }
    let config = match Mp4PlayerConfig::new(backend, video, parent_pid) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    match Mp4Player::spawn(config) {
        Ok(mut player) => loop {
            match player.try_wait() {
                Ok(Some(status)) => {
                    return ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8);
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(16)),
                Err(error) => {
                    eprintln!("Error esperando al reproductor: {error}");
                    return ExitCode::from(1);
                }
            }
        },
        Err(error) => {
            eprintln!("No se pudo iniciar el reproductor: {error}");
            ExitCode::from(1)
        }
    }
}
