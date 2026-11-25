//! Native-resolution MP4 playback contract.
//!
//! The decoder stays in the existing D3D11/libmpv worker. This Rust layer owns
//! its process lifetime and validates the media contract used by isolated
//! tests: native dimensions and source cadence are never rewritten.

use std::{
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
    process::{Child, Command},
};

const MAX_DIMENSION: u32 = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub fps_num: u64,
    pub fps_den: u64,
}

impl VideoInfo {
    pub fn fps(self) -> f64 {
        self.fps_num as f64 / self.fps_den as f64
    }

    pub fn frame_bytes(self) -> io::Result<usize> {
        (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Tamaño de cuadro inválido"))
    }

    /// Presentation must consume the source texture at its native size.
    pub fn native_viewport(self) -> [u32; 2] {
        [self.width, self.height]
    }

    fn validate(self) -> io::Result<Self> {
        if self.width == 0
            || self.height == 0
            || self.width > MAX_DIMENSION
            || self.height > MAX_DIMENSION
            || self.fps_num == 0
            || self.fps_den == 0
            || !self.fps().is_finite()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Dimensiones o cadencia MP4 fuera del contrato",
            ));
        }
        self.frame_bytes()?;
        Ok(self)
    }
}

#[derive(Debug)]
pub struct Mp4PlayerConfig {
    pub backend: PathBuf,
    pub video: PathBuf,
    pub parent_pid: u32,
}

impl Mp4PlayerConfig {
    pub fn new(
        backend: impl Into<PathBuf>,
        video: impl Into<PathBuf>,
        parent_pid: u32,
    ) -> io::Result<Self> {
        let video = video.into();
        if video
            .extension()
            .and_then(OsStr::to_str)
            .map(|ext| ext.eq_ignore_ascii_case("mp4"))
            != Some(true)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "El reproductor aislado requiere un archivo .mp4",
            ));
        }
        if parent_pid == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "PID propietario inválido",
            ));
        }
        Ok(Self {
            backend: backend.into(),
            video,
            parent_pid,
        })
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.backend);
        command
            .arg(&self.video)
            .arg(self.parent_pid.to_string())
            // Force the low-memory zero-copy profile even when a parent shell
            // has diagnostic overrides enabled.
            .env_remove("IKA_MPV_COPY")
            .env_remove("IKA_MPV_SOFTWARE")
            .env_remove("IKA_VIDEO_LOOP_FADE");
        command
    }
}

/// Rust-owned lifetime for one isolated native MP4 worker.
pub struct Mp4Player {
    child: Child,
}

impl Mp4Player {
    pub fn spawn(config: Mp4PlayerConfig) -> io::Result<Self> {
        if !config.video.is_file() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "No existe el MP4"));
        }
        if !config.backend.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "No existe el backend de video",
            ));
        }
        Ok(Self {
            child: config.command().spawn()?,
        })
    }

    pub fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        self.child.try_wait()
    }

    pub fn stop(&mut self) -> io::Result<()> {
        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
            self.child.wait()?;
        }
        Ok(())
    }
}

impl Drop for Mp4Player {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn read_u32(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(..4)?.try_into().ok()?))
}

fn read_u64(bytes: &[u8]) -> Option<u64> {
    Some(u64::from_be_bytes(bytes.get(..8)?.try_into().ok()?))
}

fn boxes(bytes: &[u8]) -> impl Iterator<Item = (&[u8; 4], &[u8])> {
    let mut offset = 0;
    std::iter::from_fn(move || {
        let header = bytes.get(offset..offset + 8)?;
        let size = read_u32(header)? as u64;
        let kind = header[4..8].try_into().ok()?;
        let header_size: usize = if size == 1 { 16 } else { 8 };
        let total = if size == 1 {
            read_u64(bytes.get(offset + 8..offset + 16)?)?
        } else {
            size
        };
        if total < header_size as u64 || total > usize::MAX as u64 {
            return None;
        }
        let end = offset.checked_add(total as usize)?;
        let payload = bytes.get(offset + header_size..end)?;
        offset = end;
        Some((kind, payload))
    })
}

fn find_payload<'a>(bytes: &'a [u8], name: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(bytes).find_map(|(kind, payload)| (kind == name).then_some(payload))
}

fn tkhd_size(payload: &[u8]) -> Option<(u32, u32)> {
    let version = *payload.first()?;
    let offset = match version {
        0 => 76,
        1 => 88,
        _ => return None,
    };
    let width = read_u32(payload.get(offset..offset + 4)?)? >> 16;
    let height = read_u32(payload.get(offset + 4..offset + 8)?)? >> 16;
    Some((width, height))
}

fn video_handler(payload: &[u8]) -> bool {
    payload.get(8..12) == Some(b"vide")
}

fn mdhd_timescale(payload: &[u8]) -> Option<u64> {
    match *payload.first()? {
        0 => Some(read_u32(payload.get(12..16)?)? as u64),
        1 => Some(read_u32(payload.get(20..24)?)? as u64),
        _ => None,
    }
}

fn stts_rate(payload: &[u8], timescale: u64) -> Option<(u64, u64)> {
    let count = read_u32(payload.get(4..8)?)? as usize;
    let mut samples = 0u64;
    let mut duration = 0u64;
    for index in 0..count {
        let start = 8usize.checked_add(index.checked_mul(8)?)?;
        let sample_count = read_u32(payload.get(start..start + 4)?)? as u64;
        let delta = read_u32(payload.get(start + 4..start + 8)?)? as u64;
        samples = samples.checked_add(sample_count)?;
        duration = duration.checked_add(sample_count.checked_mul(delta)?)?;
    }
    if samples > 0 && duration > 0 && timescale > 0 {
        Some((samples.checked_mul(timescale)?, duration))
    } else {
        None
    }
}

/// Read the first video track without decoding frames.
pub fn parse_mp4_info(bytes: &[u8]) -> io::Result<VideoInfo> {
    let moov = find_payload(bytes, b"moov")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "MP4 sin moov"))?;
    for (kind, trak) in boxes(moov) {
        if kind != b"trak" {
            continue;
        }
        let tkhd = find_payload(trak, b"tkhd").and_then(tkhd_size);
        let mdia = find_payload(trak, b"mdia");
        let Some((width, height)) = tkhd else {
            continue;
        };
        let Some(mdia) = mdia else { continue };
        let Some(hdlr) = find_payload(mdia, b"hdlr") else {
            continue;
        };
        if !video_handler(hdlr) {
            continue;
        }
        let timescale = find_payload(mdia, b"mdhd").and_then(mdhd_timescale);
        let stbl = find_payload(mdia, b"minf").and_then(|minf| find_payload(minf, b"stbl"));
        let rate = stbl
            .and_then(|stbl| find_payload(stbl, b"stts"))
            .and_then(|stts| stts_rate(stts, timescale?));
        let (fps_num, fps_den) = rate.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "MP4 sin cadencia de video")
        })?;
        return VideoInfo {
            width,
            height,
            fps_num,
            fps_den,
        }
        .validate();
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "MP4 sin pista de video",
    ))
}

pub fn validate_mp4_path(path: &Path) -> io::Result<()> {
    if path
        .extension()
        .and_then(OsStr::to_str)
        .map(|ext| ext.eq_ignore_ascii_case("mp4"))
        == Some(true)
    {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Se esperaba extensión .mp4",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    fn fixture(width: u32, height: u32, fps: u32) -> Vec<u8> {
        let mut tkhd = vec![0u8; 76 + 8];
        tkhd[0] = 0;
        tkhd[76..80].copy_from_slice(&(width << 16).to_be_bytes());
        tkhd[80..84].copy_from_slice(&(height << 16).to_be_bytes());
        let mut hdlr = vec![0u8; 12];
        hdlr[8..12].copy_from_slice(b"vide");
        let mut mdhd = vec![0u8; 16];
        mdhd[12..16].copy_from_slice(&6000u32.to_be_bytes());
        let mut stts = Vec::new();
        stts.extend_from_slice(&0u32.to_be_bytes());
        stts.extend_from_slice(&1u32.to_be_bytes());
        stts.extend_from_slice(&fps.to_be_bytes());
        stts.extend_from_slice(&((6000 / fps) as u32).to_be_bytes());
        let stbl = atom(b"stbl", &atom(b"stts", &stts));
        let minf = atom(b"minf", &stbl);
        let mdia = atom(
            b"mdia",
            &[atom(b"mdhd", &mdhd), atom(b"hdlr", &hdlr), minf].concat(),
        );
        let trak = atom(b"trak", &[atom(b"tkhd", &tkhd), mdia].concat());
        atom(b"ftyp", b"isom")
            .into_iter()
            .chain(atom(b"moov", &trak))
            .collect()
    }

    #[test]
    fn preserves_1080p_24fps() {
        let info = parse_mp4_info(&fixture(1920, 1080, 24)).unwrap();
        assert_eq!(info.native_viewport(), [1920, 1080]);
        assert!((info.fps() - 24.0).abs() < 0.01);
    }

    #[test]
    fn preserves_1440p_60fps() {
        let info = parse_mp4_info(&fixture(2560, 1440, 60)).unwrap();
        assert_eq!(info.native_viewport(), [2560, 1440]);
        assert!((info.fps() - 60.0).abs() < 0.01);
    }

    #[test]
    fn preserves_4k_30fps_and_memory_is_bounded() {
        let info = parse_mp4_info(&fixture(3840, 2160, 30)).unwrap();
        assert_eq!(info.native_viewport(), [3840, 2160]);
        assert!((info.fps() - 30.0).abs() < 0.01);
        assert_eq!(info.frame_bytes().unwrap(), 33_177_600);
    }

    #[test]
    fn rejects_non_mp4_and_bad_dimensions() {
        assert!(validate_mp4_path(Path::new("clip.mkv")).is_err());
        assert!(
            VideoInfo {
                width: 0,
                height: 1080,
                fps_num: 60,
                fps_den: 1
            }
            .validate()
            .is_err()
        );
        assert!(
            VideoInfo {
                width: 8193,
                height: 2160,
                fps_num: 60,
                fps_den: 1
            }
            .validate()
            .is_err()
        );
    }
}
