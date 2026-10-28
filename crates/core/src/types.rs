use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type DownloadId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeInfo {
    pub start: u64,
    pub end: u64,
    pub pos: u64,
}

/// Lifecycle of a download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    Queued,
    Probing,
    Running,
    Paused,
    Completing,
    Done,
    Error,
    Canceled,
}

impl DownloadState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Probing => "probing",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Completing => "completing",
            Self::Done => "done",
            Self::Error => "error",
            Self::Canceled => "canceled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "queued" => Self::Queued,
            "probing" => Self::Probing,
            "running" => Self::Running,
            "paused" => Self::Paused,
            "completing" => Self::Completing,
            "done" => Self::Done,
            "error" => Self::Error,
            "canceled" => Self::Canceled,
            _ => return None,
        })
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Probing | Self::Running | Self::Completing)
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Error | Self::Canceled)
    }

    pub fn is_unfinished(self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Probing | Self::Running | Self::Paused
        )
    }
}

impl std::fmt::Display for DownloadState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// User-supplied description of a download to create.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DownloadSpec {
    pub url: String,
    pub dir: Option<String>,
    pub filename: Option<String>,
    pub category: Option<String>,
    pub connections: Option<u32>,
    /// Bytes per second. 0 = unlimited.
    pub speed_limit: u64,
    pub priority: i32,
    pub headers: HashMap<String, String>,
    pub cookie: Option<String>,
    pub user_agent: Option<String>,
    pub proxy: Option<String>,
    pub start_now: bool,
}

impl Default for DownloadSpec {
    fn default() -> Self {
        Self {
            url: String::new(),
            dir: None,
            filename: None,
            category: None,
            connections: None,
            speed_limit: 0,
            priority: 0,
            headers: HashMap::new(),
            cookie: None,
            user_agent: None,
            proxy: None,
            start_now: true,
        }
    }
}

/// Persisted download row, shared between the database and the engine.
#[derive(Debug, Clone)]
pub struct DownloadRecord {
    pub id: DownloadId,
    pub url: String,
    pub final_url: Option<String>,
    pub filename: String,
    pub dir: String,
    pub total_size: u64,
    pub received: u64,
    pub state: DownloadState,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_type: Option<String>,
    pub accept_ranges: bool,
    pub connections: u32,
    pub speed_limit: u64,
    pub category: Option<String>,
    pub priority: i32,
    pub error: Option<String>,
    pub sha256: Option<String>,
    pub headers: HashMap<String, String>,
    pub user_agent: Option<String>,
    pub cookie: Option<String>,
    pub proxy: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
}

impl DownloadRecord {
    pub fn final_path(&self) -> std::path::PathBuf {
        std::path::Path::new(&self.dir).join(&self.filename)
    }

    /// Sidecar file that receives the bytes while the download is running.
    pub fn part_path(&self) -> std::path::PathBuf {
        let mut name = self.filename.clone();
        name.push_str(".hfpart");
        std::path::Path::new(&self.dir).join(name)
    }

    /// All HTTP headers that should accompany every request for this download.
    pub fn request_headers(&self) -> HashMap<String, String> {
        let mut headers = self.headers.clone();
        if let Some(cookie) = &self.cookie {
            headers.insert("Cookie".to_string(), cookie.clone());
        }
        if let Some(ua) = &self.user_agent {
            headers.insert("User-Agent".to_string(), ua.clone());
        }
        headers
    }
}

/// Serialisable presentation of a download, sent to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadSnapshot {
    pub id: DownloadId,
    pub url: String,
    pub final_url: Option<String>,
    pub filename: String,
    pub dir: String,
    pub total_size: u64,
    pub received: u64,
    pub state: DownloadState,
    pub accept_ranges: bool,
    pub connections: u32,
    pub speed_limit: u64,
    pub category: Option<String>,
    pub priority: i32,
    pub error: Option<String>,
    pub sha256: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub finished_at: Option<i64>,
    pub speed: u64,
    pub eta_secs: Option<u64>,
    pub active_connections: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub id: DownloadId,
    pub received: u64,
    pub total: u64,
    pub speed: u64,
    pub eta_secs: Option<u64>,
    pub active_connections: u32,
}

/// High-level file categories used for automatic folder placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Video,
    Audio,
    Documents,
    Archives,
    Programs,
    Images,
    Other,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::Documents => "Documents",
            Self::Archives => "Archives",
            Self::Programs => "Programs",
            Self::Images => "Images",
            Self::Other => "Other",
        }
    }

    pub fn from_extension(extension: &str) -> Self {
        let extension = extension.to_ascii_lowercase();
        match extension.as_str() {
            "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "mpg" | "mpeg"
            | "ts" | "m3u8" | "mpd" => Self::Video,
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "opus" | "wma" => Self::Audio,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "epub"
            | "mobi" | "csv" | "md" => Self::Documents,
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso" | "zst" => Self::Archives,
            "exe" | "msi" | "msix" | "appx" | "apk" | "dmg" | "deb" | "rpm" | "pkg" => {
                Self::Programs
            }
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "svg" | "tiff" | "heic" => {
                Self::Images
            }
            _ => Self::Other,
        }
    }
}

/// Engine settings. Persisted as JSON inside the database and editable from the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub download_dir: String,
    pub max_concurrent: usize,
    pub connections_per_download: u32,
    pub max_connections: u32,
    /// Size of a single range request batch (bytes).
    pub batch_size: u64,
    /// Smallest range worth splitting into two for rebalancing.
    pub min_split_size: u64,
    /// Files smaller than this fall back to a single connection.
    pub min_segmented_size: u64,
    /// Global speed limit in bytes/second. 0 = unlimited.
    pub global_speed_limit: u64,
    pub auto_resume: bool,
    /// Retries applied to a failed batch before the download errors out.
    pub batch_retries: u32,
    pub compute_sha256: bool,
    /// Force file data to disk before the final rename (slower, safer on power loss).
    pub durable_finish: bool,
    pub categorize: bool,
    /// Custom folder per category. Values are relative to `download_dir`.
    pub category_folders: HashMap<String, String>,
    pub proxy: Option<String>,
    pub user_agent: String,
    pub clipboard_monitor: bool,
    pub clipboard_auto_add: bool,
    pub confirm_captures: bool,
    pub bridge_port: u16,
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        let download_dir = default_download_dir();
        let mut category_folders = HashMap::new();
        for category in [
            Category::Video,
            Category::Audio,
            Category::Documents,
            Category::Archives,
            Category::Programs,
            Category::Images,
            Category::Other,
        ] {
            category_folders.insert(category.label().to_string(), category.label().to_string());
        }

        Self {
            download_dir,
            max_concurrent: 3,
            connections_per_download: 8,
            max_connections: 32,
            batch_size: 1 << 20,
            min_split_size: 2 << 20,
            min_segmented_size: 4 << 20,
            global_speed_limit: 0,
            auto_resume: true,
            batch_retries: 5,
            compute_sha256: false,
            durable_finish: false,
            categorize: true,
            category_folders,
            proxy: None,
            user_agent: format!("HyperFetch/{}", env!("CARGO_PKG_VERSION")),
            clipboard_monitor: true,
            clipboard_auto_add: true,
            confirm_captures: true,
            bridge_port: 47821,
            theme: "classic".to_string(),
        }
    }
}

fn default_download_dir() -> String {
    #[cfg(target_os = "windows")]
    {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            return format!("{profile}\\Downloads\\HyperFetch");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return format!("{home}/Downloads/HyperFetch");
    }
    std::env::temp_dir()
        .join("HyperFetch")
        .to_string_lossy()
        .into_owned()
}
