use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hyperfetch_core::util::{filename_from_url, now_ms};
use hyperfetch_core::{DownloadSpec, Engine, ProbeResult};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_clipboard_manager::ClipboardExt;
use url::Url;
use uuid::Uuid;

const POLL_INTERVAL: Duration = Duration::from_millis(700);
const MAX_TEXT_LEN: usize = 8 * 1024;
const MAX_RECENT: usize = 200;
const MAX_OFFERS: usize = 8;

#[derive(Debug, Clone, Serialize)]
pub struct ClipboardOffer {
    pub id: Uuid,
    pub url: String,
    pub filename: String,
    pub total_size: u64,
    pub content_type: Option<String>,
    pub kind: String,
    pub auto: bool,
    pub created_at: i64,
}

#[derive(Default)]
pub struct ClipboardState {
    offers: Mutex<HashMap<Uuid, ClipboardOffer>>,
    recent: Mutex<VecDeque<String>>,
}

impl ClipboardState {
    pub fn list(&self) -> Vec<ClipboardOffer> {
        let offers = self.offers.lock().expect("clipboard offers poisoned");
        let mut values: Vec<ClipboardOffer> = offers.values().cloned().collect();
        values.sort_by_key(|offer| offer.created_at);
        values
    }

    pub fn get(&self, id: Uuid) -> Option<ClipboardOffer> {
        self.offers
            .lock()
            .expect("clipboard offers poisoned")
            .get(&id)
            .cloned()
    }

    pub fn remove(&self, id: Uuid) -> Option<ClipboardOffer> {
        self.offers
            .lock()
            .expect("clipboard offers poisoned")
            .remove(&id)
    }

    pub fn put(&self, offer: ClipboardOffer) {
        let mut offers = self.offers.lock().expect("clipboard offers poisoned");
        if offers.len() >= MAX_OFFERS {
            if let Some(oldest) = offers
                .values()
                .min_by_key(|offer| offer.created_at)
                .map(|offer| offer.id)
            {
                offers.remove(&oldest);
            }
        }
        offers.insert(offer.id, offer);
    }

    fn seen(&self, url: &str) -> bool {
        self.recent
            .lock()
            .expect("clipboard history poisoned")
            .iter()
            .any(|item| item == url)
    }

    fn remember(&self, url: String) {
        let mut recent = self.recent.lock().expect("clipboard history poisoned");
        if recent.iter().any(|item| item == &url) {
            return;
        }
        if recent.len() >= MAX_RECENT {
            recent.pop_front();
        }
        recent.push_back(url);
    }
}

/// Start the background clipboard watcher. Reading happens entirely on-device.
pub fn spawn(app: AppHandle, engine: Engine, state: Arc<ClipboardState>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(POLL_INTERVAL);
        let mut last_text = String::new();
        loop {
            interval.tick().await;
            let settings = engine.settings().await;
            if !settings.clipboard_monitor {
                continue;
            }
            let text = match app.clipboard().read_text() {
                Ok(text) => text,
                Err(_) => continue,
            };
            if text == last_text {
                continue;
            }
            last_text = text.clone();
            if text.len() > MAX_TEXT_LEN {
                continue;
            }
            let Some(url) = copied_url(&text) else {
                continue;
            };
            if state.seen(&url) {
                continue;
            }
            if matches!(engine.url_exists(&url), Ok(true)) {
                state.remember(url);
                continue;
            }

            if looks_like_hls(&url) {
                let filename = Url::parse(&url)
                    .ok()
                    .and_then(|parsed| filename_from_url(&parsed))
                    .unwrap_or_else(|| "video.ts".to_string());
                let offer = ClipboardOffer {
                    id: Uuid::new_v4(),
                    url,
                    filename,
                    total_size: 0,
                    content_type: None,
                    kind: "hls".to_string(),
                    auto: false,
                    created_at: now_ms(),
                };
                state.remember(offer.url.clone());
                state.put(offer.clone());
                let _ = app.emit("clipboard://offer", &offer);
                continue;
            }

            let probe = match engine.probe(&url, None, None).await {
                Ok(probe) => probe,
                Err(error) => {
                    tracing::debug!(%url, %error, "clipboard probe failed");
                    continue;
                }
            };
            state.remember(url.clone());

            if is_web_page(&probe) {
                continue;
            }
            if !is_download(&url, &probe) {
                continue;
            }

            if settings.clipboard_auto_add {
                let spec = DownloadSpec {
                    url: url.clone(),
                    filename: Some(probe.filename.clone()),
                    start_now: true,
                    ..Default::default()
                };
                match engine.add(spec).await {
                    Ok(snapshot) => {
                        let offer = ClipboardOffer {
                            id: snapshot.id,
                            url,
                            filename: snapshot.filename,
                            total_size: probe.total_size,
                            content_type: probe.content_type.clone(),
                            kind: "download".to_string(),
                            auto: true,
                            created_at: now_ms(),
                        };
                        let _ = app.emit("clipboard://offer", &offer);
                    }
                    Err(error) => {
                        tracing::warn!(url = %url, %error, "clipboard auto-add failed");
                    }
                }
                continue;
            }

            let offer = ClipboardOffer {
                id: Uuid::new_v4(),
                url,
                filename: probe.filename.clone(),
                total_size: probe.total_size,
                content_type: probe.content_type.clone(),
                kind: "download".to_string(),
                auto: false,
                created_at: now_ms(),
            };
            state.put(offer.clone());
            let _ = app.emit("clipboard://offer", &offer);
        }
    });
}

pub fn copied_url(text: &str) -> Option<String> {
    let trimmed = text.trim().trim_matches(|c| {
        matches!(
            c,
            '<' | '>' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}'
        )
    });
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return None;
    }
    let token = trimmed.split_whitespace().next()?;
    let token = token.trim_end_matches(|c: char| {
        matches!(
            c,
            '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '\'' | '"'
        )
    });
    let mut parsed = Url::parse(token).ok()?;
    if parsed.host_str()?.is_empty() {
        return None;
    }
    parsed.set_fragment(None);
    if parsed.path().starts_with("/v1/") {
        return None;
    }
    Some(parsed.to_string())
}

pub fn looks_like_hls(url: &str) -> bool {
    url.to_ascii_lowercase().contains(".m3u8")
}

pub fn is_web_page(probe: &ProbeResult) -> bool {
    let content_type = probe
        .content_type
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    if content_type.contains("text/html") || content_type.contains("application/xhtml") {
        return !is_attachment(probe);
    }
    false
}

pub fn is_download(url: &str, probe: &ProbeResult) -> bool {
    if is_attachment(probe) {
        return true;
    }
    if extension_is_known(&probe.filename) {
        return true;
    }
    if let Ok(parsed) = Url::parse(url) {
        if let Some(name) = filename_from_url(&parsed) {
            if extension_is_known(&name) {
                return true;
            }
        }
    }
    let content_type = probe
        .content_type
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    if content_type.is_empty() {
        return probe.total_size > 0 && probe.accept_ranges;
    }
    if content_type.starts_with("text/") || content_type.contains("json") {
        return false;
    }
    content_type.starts_with("application/")
        || content_type.starts_with("video/")
        || content_type.starts_with("audio/")
        || content_type.starts_with("image/")
        || content_type.starts_with("font/")
}

fn is_attachment(probe: &ProbeResult) -> bool {
    probe
        .content_disposition
        .as_deref()
        .map(|value| {
            value
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("attachment")
        })
        .unwrap_or(false)
}

fn extension_is_known(filename: &str) -> bool {
    let extension = Path::new(filename)
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    matches!(
        extension.as_str(),
        "7z" | "apk"
            | "appx"
            | "avi"
            | "bin"
            | "bz2"
            | "cab"
            | "csv"
            | "deb"
            | "dmg"
            | "doc"
            | "docx"
            | "epub"
            | "exe"
            | "flac"
            | "flv"
            | "gz"
            | "heic"
            | "img"
            | "iso"
            | "jar"
            | "m4a"
            | "m4v"
            | "mkv"
            | "mov"
            | "mp3"
            | "mp4"
            | "mpeg"
            | "mpg"
            | "msi"
            | "msix"
            | "ogg"
            | "opus"
            | "pdf"
            | "pkg"
            | "ppt"
            | "pptx"
            | "rar"
            | "rpm"
            | "tar"
            | "tgz"
            | "ts"
            | "wav"
            | "webm"
            | "webp"
            | "wma"
            | "wmv"
            | "xls"
            | "xlsx"
            | "xz"
            | "zip"
            | "zst"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(filename: &str, content_type: &str, disposition: Option<&str>) -> ProbeResult {
        ProbeResult {
            final_url: "https://example.com/file".into(),
            filename: filename.into(),
            total_size: 1024,
            accept_ranges: true,
            etag: None,
            last_modified: None,
            content_type: Some(content_type.into()),
            content_disposition: disposition.map(str::to_string),
        }
    }

    #[test]
    fn extracts_only_standalone_http_links() {
        assert_eq!(
            copied_url("  https://example.com/files/app.zip?x=1#frag  ").as_deref(),
            Some("https://example.com/files/app.zip?x=1")
        );
        assert_eq!(copied_url("see https://example.com/file.zip"), None);
        assert_eq!(copied_url("hello world"), None);
        assert_eq!(copied_url("http://127.0.0.1:1234/v1/health"), None);
    }

    #[test]
    fn classifies_attachments_and_binary_types() {
        assert!(is_download(
            "https://example.com/download",
            &probe("file", "application/octet-stream", None)
        ));
        assert!(is_download(
            "https://example.com/get",
            &probe("get", "text/html", Some("attachment; filename=setup.exe"))
        ));
        assert!(!is_download(
            "https://example.com/page",
            &probe("page", "text/html", None)
        ));
        assert!(!is_download(
            "https://example.com/api",
            &probe("api", "application/json", None)
        ));
        assert!(is_download(
            "https://example.com/archive",
            &probe("archive", "", None)
        ));
        assert!(is_download(
            "https://example.com/file.ZIP",
            &probe("file.ZIP", "", None)
        ));
    }

    #[test]
    fn recognizes_hls_links() {
        assert!(looks_like_hls("https://example.com/master.m3u8?token=x"));
        assert!(!looks_like_hls("https://example.com/video.mp4"));
    }
}
