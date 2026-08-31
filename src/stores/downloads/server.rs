//! Minimal localhost HTTP server for serving downloaded media to the
//! desktop WebView (Linux desktop only).
//!
//! The WebView `<audio>` element cannot load `file://` URLs from the secure
//! `dioxus://` origin, and wry's custom-protocol request path does not
//! reliably surface `Range` headers on webkit2gtk — so seeking is only
//! guaranteed over real HTTP. This server binds `127.0.0.1:{ephemeral}` and
//! serves files under the media dir with full Range/206 support. It runs on
//! a dedicated std thread (local-file I/O, no async runtime needed).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

static PORT: AtomicU16 = AtomicU16::new(0);
static STARTING: AtomicBool = AtomicBool::new(false);
static MEDIA_ROOT: OnceLock<PathBuf> = OnceLock::new();

const READ_TIMEOUT: Duration = Duration::from_secs(10);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const CHUNK: usize = 64 * 1024;
const MAX_HEADER_BYTES: usize = 8 * 1024;

/// Ensure the server is running, returning its port. Returns `None` when the
/// media root cannot be determined or binding fails.
pub fn ensure_started() -> Option<u16> {
    let port = PORT.load(Ordering::SeqCst);
    if port != 0 {
        return Some(port);
    }
    if STARTING.swap(true, Ordering::SeqCst) {
        // Another thread is binding; wait briefly for it.
        for _ in 0..50 {
            let port = PORT.load(Ordering::SeqCst);
            if port != 0 {
                return Some(port);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        return None;
    }
    let root = super::resolver::media_dir();
    if std::fs::create_dir_all(&root).is_err() {
        log::error!("Failed to create media dir: {}", root.display());
        STARTING.store(false, Ordering::SeqCst);
        return None;
    }
    let listener = match TcpListener::bind(("127.0.0.1", 0)) {
        Ok(l) => l,
        Err(e) => {
            log::error!("Failed to bind media server: {}", e);
            STARTING.store(false, Ordering::SeqCst);
            return None;
        }
    };
    let port = listener.local_addr().ok()?.port();
    let _ = MEDIA_ROOT.set(root);
    PORT.store(port, Ordering::SeqCst);
    std::thread::Builder::new()
        .name("media-server".to_string())
        .spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        // Per-connection thread: each response streams the
                        // entire file before returning, so inline handling
                        // would head-of-line-block the parallel range probes
                        // the WebView's <audio> element issues (a slow client
                        // would stall every other player). handle_conn only
                        // touches the read-only MEDIA_ROOT and its own stream.
                        std::thread::spawn(move || handle_conn(stream));
                    }
                    Err(e) => {
                        log::warn!("Media server accept error: {}", e);
                    }
                }
            }
        })
        .ok()?;
    log::info!("Media server listening on 127.0.0.1:{}", port);
    Some(port)
}

fn handle_conn(mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    // Bound writes too: a stalled reader must not wedge the connection
    // thread indefinitely (the read timeout alone doesn't cover the
    // file-streaming direction).
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let Some((method, path, range_header)) = read_request(&mut stream) else {
        return;
    };
    if method != "GET" && method != "HEAD" {
        write_simple(&mut stream, 405, "Method Not Allowed", b"");
        return;
    }
    let Some(root) = MEDIA_ROOT.get() else {
        write_simple(&mut stream, 500, "Internal Server Error", b"");
        return;
    };
    let Some(file_path) = resolve_path(root, &path) else {
        write_simple(&mut stream, 404, "Not Found", b"");
        return;
    };
    let Ok(meta) = std::fs::metadata(&file_path) else {
        write_simple(&mut stream, 404, "Not Found", b"");
        return;
    };
    if !meta.is_file() {
        write_simple(&mut stream, 404, "Not Found", b"");
        return;
    }
    let total = meta.len();
    if total == 0 {
        // Empty file: advertise zero bytes up front. Computing a range over
        // a zero-length file yields a phantom Content-Length of 1 that the
        // send loop can never deliver, leaving the client waiting.
        write_simple(&mut stream, 200, "OK", b"");
        return;
    }
    let mime = content_type(&file_path);

    let (status, start, end) = match parse_range(&range_header, total) {
        Some(range) => range,
        None => (200, 0, total.saturating_sub(1)),
    };
    let length = end - start + 1;

    let mut header = Vec::with_capacity(256);
    let status_line = match status {
        206 => "HTTP/1.1 206 Partial Content",
        416 => "HTTP/1.1 416 Range Not Satisfiable",
        _ => "HTTP/1.1 200 OK",
    };
    header.extend_from_slice(status_line.as_bytes());
    header.extend_from_slice(b"\r\nContent-Type: ");
    header.extend_from_slice(mime.as_bytes());
    header.extend_from_slice(b"\r\nAccept-Ranges: bytes");
    if status == 416 {
        header.extend_from_slice(
            format!("\r\nContent-Range: bytes */{total}\r\nContent-Length: 0").as_bytes(),
        );
        let _ = stream.write_all(&header);
        let _ = stream.write_all(b"\r\n\r\n");
        return;
    }
    if status == 206 {
        header.extend_from_slice(
            format!("\r\nContent-Range: bytes {start}-{end}/{total}").as_bytes(),
        );
    }
    header.extend_from_slice(format!("\r\nContent-Length: {length}").as_bytes());
    header.extend_from_slice(b"\r\nConnection: close\r\n\r\n");
    if stream.write_all(&header).is_err() {
        return;
    }
    if method == "HEAD" {
        return;
    }
    let Ok(mut file) = std::fs::File::open(&file_path) else {
        return;
    };
    if std::io::Seek::seek(&mut file, std::io::SeekFrom::Start(start)).is_err() {
        return;
    }
    let mut remaining = length as usize;
    let mut buf = vec![0u8; CHUNK];
    while remaining > 0 {
        let to_read = remaining.min(CHUNK);
        match file.read(&mut buf[..to_read]) {
            Ok(0) => break,
            Ok(n) => {
                if stream.write_all(&buf[..n]).is_err() {
                    break;
                }
                remaining -= n;
            }
            Err(_) => break,
        }
    }
    let _ = stream.flush();
}

/// Read one HTTP request; returns `(method, path, range_header)`.
fn read_request(stream: &mut TcpStream) -> Option<(String, String, String)> {
    let mut buf = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    loop {
        if buf.len() > MAX_HEADER_BYTES {
            return None;
        }
        match stream.read(&mut byte) {
            Ok(0) => return None,
            Ok(_) => {
                buf.push(byte[0]);
                if buf.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            Err(_) => return None,
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.lines();
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let path = target.split(['?', '#']).next().unwrap_or("").to_string();
    let mut range = String::new();
    for line in lines {
        if let Some(value) = line
            .split_once(':')
            .filter(|(name, _)| name.trim().eq_ignore_ascii_case("range"))
            .map(|(_, value)| value.trim())
        {
            range = value.to_string();
        }
    }
    Some((method, path, range))
}

/// Map a URL path to a file inside the media root, rejecting traversal.
fn resolve_path(root: &Path, url_path: &str) -> Option<PathBuf> {
    let decoded = urlencoding::decode(url_path.trim_start_matches('/')).ok()?;
    let decoded = decoded.as_ref();
    if decoded.contains("..") || decoded.contains('\\') || decoded.starts_with('/') {
        return None;
    }
    let candidate = root.join(decoded);
    // Defense in depth: canonicalize and verify containment.
    let canonical = candidate.canonicalize().ok()?;
    let canonical_root = root.canonicalize().ok()?;
    if canonical.starts_with(&canonical_root) {
        Some(canonical)
    } else {
        None
    }
}

/// Parse a `Range: bytes=a-b` header. `None` means serve the whole file.
/// Returns `(status, start, end)` where end is inclusive.
fn parse_range(header: &str, total: u64) -> Option<(u16, u64, u64)> {
    if header.is_empty() || total == 0 {
        return None;
    }
    let spec = header.trim().strip_prefix("bytes=")?;
    let (start_str, end_str) = spec.split_once('-')?;
    let (start, end) = if start_str.is_empty() {
        // Suffix range: last N bytes.
        let n: u64 = end_str.trim().parse().ok()?;
        let start = total.saturating_sub(n);
        (start, total - 1)
    } else {
        let start: u64 = start_str.trim().parse().ok()?;
        let end = if end_str.trim().is_empty() {
            total - 1
        } else {
            end_str.trim().parse::<u64>().ok()?.min(total - 1)
        };
        (start, end)
    };
    if start >= total {
        return Some((416, 0, 0));
    }
    Some((206, start, end))
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "mp4" => "audio/mp4",
        "aac" => "audio/aac",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "webm" => "audio/webm",
        _ => "application/octet-stream",
    }
}

fn write_simple(stream: &mut TcpStream, status: u16, reason: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_range_full() {
        assert_eq!(parse_range("", 1000), None);
        assert_eq!(parse_range("bytes=0-499", 1000), Some((206, 0, 499)));
        assert_eq!(parse_range("bytes=500-", 1000), Some((206, 500, 999)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((206, 900, 999)));
        // Unsatisfiable.
        assert_eq!(parse_range("bytes=1000-", 1000), Some((416, 0, 0)));
        // End clamped to total-1.
        assert_eq!(parse_range("bytes=0-2000", 1000), Some((206, 0, 999)));
    }

    #[test]
    fn test_resolve_path_rejects_traversal() {
        let root = std::env::temp_dir();
        assert!(resolve_path(&root, "../etc/passwd").is_none());
        assert!(resolve_path(&root, "a/../../etc/passwd").is_none());
        assert!(resolve_path(&root, "/etc/passwd").is_none());
    }

    #[test]
    fn test_content_type() {
        assert_eq!(
            content_type(Path::new("/x/y/episode.mp3")),
            "audio/mpeg"
        );
        assert_eq!(content_type(Path::new("/x/y/file")), "application/octet-stream");
    }
}
