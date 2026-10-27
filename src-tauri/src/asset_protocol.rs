//! The `asset:` protocol (convertFileSrc: background videos, wallpaper previews, instance icons,
//! skins...), served off the UI thread.
//!
//! Tauri's own handler reads the file inside the WebView's request callback, which on Windows runs
//! on the window's thread, and without a Range header it reads the whole file: a several-MB
//! Workshop preview.gif or a video chunk froze the entire launcher while the disk answered. This
//! one does the same work (whole file, or one range of at most RANGE_MAX bytes) on Tauri's
//! blocking thread pool and answers when it is read. Registered before the window is created, it
//! replaces Tauri's (which is only added when no "asset" handler exists).

use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use tauri::http::{header, Request, Response, StatusCode};

/// The largest piece answered to one range request (as Tauri's): the answer is still handed to
/// the WebView on the UI thread, so pieces stay small.
const RANGE_MAX: u64 = 1000 * 1024;

pub fn handle(request: Request<Vec<u8>>, responder: tauri::UriSchemeResponder) {
    tauri::async_runtime::spawn_blocking(move || {
        let response = respond(&request).unwrap_or_else(|status| {
            Response::builder()
                .status(status)
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .body(Vec::new())
                .unwrap()
        });
        responder.respond(response);
    });
}

fn respond(request: &Request<Vec<u8>>) -> Result<Response<Vec<u8>>, StatusCode> {
    let path = decode_path(request.uri().path());
    let mut file = std::fs::File::open(&path).map_err(|_| StatusCode::NOT_FOUND)?;
    let len = file.metadata().map_err(|_| StatusCode::NOT_FOUND)?.len();
    let mime = mime_for(&path);
    let builder = Response::builder()
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes");

    let range = request
        .headers()
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| parse_range(v, len));
    match range {
        Some((start, end)) => {
            let end = end.min(start + RANGE_MAX - 1);
            let mut buf = vec![0u8; (end - start + 1) as usize];
            file.seek(SeekFrom::Start(start)).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            file.read_exact(&mut buf).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            builder
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
                .header(header::ACCESS_CONTROL_EXPOSE_HEADERS, "content-range")
                .header(header::CONTENT_LENGTH, buf.len())
                .body(buf)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        }
        None if request.headers().contains_key(header::RANGE) => Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(header::CONTENT_RANGE, format!("bytes */{len}"))
            .body(Vec::new())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR),
        None => {
            let mut buf = Vec::with_capacity(len as usize);
            file.read_to_end(&mut buf).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            builder
                .status(StatusCode::OK)
                .header(header::CONTENT_LENGTH, buf.len())
                .body(buf)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// `/C%3A%5CUsers%5C...` -> `C:\Users\...` (percent-decoded, without the leading slash).
fn decode_path(raw: &str) -> PathBuf {
    let bytes = raw.strip_prefix('/').unwrap_or(raw).as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    PathBuf::from(String::from_utf8_lossy(&out).into_owned())
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// First range of a `bytes=` header, as inclusive (start, end) within the file.
fn parse_range(value: &str, len: u64) -> Option<(u64, u64)> {
    let spec = value.trim().strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    if len == 0 {
        return None;
    }
    let (start, end) = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        (len.saturating_sub(n), len - 1)
    } else {
        let start: u64 = a.parse().ok()?;
        let end = if b.is_empty() { len - 1 } else { b.parse::<u64>().ok()?.min(len - 1) };
        (start, end)
    };
    (start <= end && start < len).then_some((start, end))
}

fn mime_for(path: &std::path::Path) -> &'static str {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "gif" => "image/gif",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "json" | "mcmeta" => "application/json",
        "txt" | "log" | "cfg" | "toml" | "properties" => "text/plain",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "text/javascript",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
}
