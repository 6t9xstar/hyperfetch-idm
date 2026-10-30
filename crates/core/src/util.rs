use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use percent_encoding::percent_decode_str;
use url::Url;

use crate::types::Settings;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// Make a remote filename safe for every supported platform.
pub fn sanitize_filename(input: &str) -> String {
    let mut name: String = input
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect();

    name = name.trim().trim_end_matches('.').trim_end().to_string();
    if name.is_empty() {
        name = "download".to_string();
    }

    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.as_str()) {
        name = format!("_{name}");
    }

    // Windows path limit safety: keep the final component well under 255 bytes.
    if name.len() > 180 {
        let extension = Path::new(&name)
            .extension()
            .map(|extension| format!(".{}", extension.to_string_lossy()))
            .unwrap_or_default();
        let stem_len = 180usize.saturating_sub(extension.len());
        let stem: String = name.chars().take(stem_len).collect();
        name = format!("{stem}{extension}");
    }

    name
}

/// Best-effort filename from a URL's path.
pub fn filename_from_url(url: &Url) -> Option<String> {
    let segments: Vec<_> = url.path_segments()?.collect();
    let last = segments
        .into_iter()
        .rev()
        .find(|segment| !segment.is_empty())?;
    let decoded = percent_decode_str(last)
        .decode_utf8_lossy()
        .trim()
        .to_string();
    if decoded.is_empty() {
        None
    } else {
        Some(sanitize_filename(&decoded))
    }
}

/// Parse a `Content-Disposition` header, preferring RFC 5987 `filename*`.
pub fn filename_from_content_disposition(value: &str) -> Option<String> {
    let mut plain: Option<String> = None;

    for part in value.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("filename*=") {
            let rest = rest.trim().trim_matches('"');
            if let Some(encoded) = rest.split("''").nth(1) {
                let decoded = percent_decode_str(encoded).decode_utf8_lossy();
                let decoded = decoded.trim();
                if !decoded.is_empty() {
                    return Some(sanitize_filename(decoded));
                }
            }
        } else if let Some(rest) = part.strip_prefix("filename=") {
            let rest = rest.trim().trim_matches('"').trim();
            if !rest.is_empty() {
                plain = Some(sanitize_filename(rest));
            }
        }
    }

    plain
}

fn part_sidecar(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".hfpart");
    PathBuf::from(os)
}

/// Pick a destination path that does not collide with existing files (or stale
/// partial files from another download).
pub fn unique_destination(dir: &Path, filename: &str) -> PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() && !part_sidecar(&candidate).exists() {
        return candidate;
    }

    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".to_string());
    let extension = path
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();

    for counter in 1..10_000u32 {
        let candidate = dir.join(format!("{stem} ({counter}){extension}"));
        if !candidate.exists() && !part_sidecar(&candidate).exists() {
            return candidate;
        }
    }

    dir.join(format!("{stem}-{}{extension}", now_ms()))
}

pub fn extension_of(filename: &str) -> String {
    Path::new(filename)
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Resolve the destination folder, applying category folders when enabled.
pub fn resolve_download_dir(
    settings: &Settings,
    requested_dir: Option<&str>,
    category: Option<&str>,
    filename: &str,
) -> (PathBuf, Option<String>) {
    if let Some(dir) = requested_dir {
        return (PathBuf::from(dir), category.map(str::to_string));
    }

    let base = PathBuf::from(&settings.download_dir);
    if !settings.categorize {
        return (base, category.map(str::to_string));
    }

    let category = category.map(str::to_string).unwrap_or_else(|| {
        crate::types::Category::from_extension(&extension_of(filename))
            .label()
            .to_string()
    });

    let folder = settings
        .category_folders
        .get(&category)
        .cloned()
        .filter(|folder| !folder.trim().is_empty())
        .unwrap_or_else(|| category.clone());

    (base.join(folder), Some(category))
}

/// Determine whether remote metadata indicates the file changed since the
/// partial download was persisted.
pub fn validators_differ(
    existing_etag: Option<&str>,
    existing_last_modified: Option<&str>,
    probed_etag: Option<&str>,
    probed_last_modified: Option<&str>,
) -> bool {
    if let (Some(a), Some(b)) = (existing_etag, probed_etag) {
        if a != b {
            return true;
        }
    }
    if existing_etag.is_some() || probed_etag.is_some() {
        return false;
    }
    match (existing_last_modified, probed_last_modified) {
        (Some(a), Some(b)) => a != b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn sanitizes_windows_reserved_names() {
        assert_eq!(sanitize_filename("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_filename("a<b>.zip"), "a_b_.zip");
        assert_eq!(sanitize_filename("   "), "download");
        assert_eq!(sanitize_filename("../../etc/passwd"), ".._.._etc_passwd");
    }

    #[test]
    fn parses_content_disposition() {
        assert_eq!(
            filename_from_content_disposition("attachment; filename=\"report.pdf\""),
            Some("report.pdf".to_string())
        );
        assert_eq!(
            filename_from_content_disposition("attachment; filename*=UTF-8''my%20file%20name.zip"),
            Some("my file name.zip".to_string())
        );
    }

    #[test]
    fn derives_filename_from_url() {
        let url = Url::parse("https://example.com/files/My%20Video.mp4?token=1").unwrap();
        assert_eq!(filename_from_url(&url), Some("My Video.mp4".to_string()));
        let url = Url::parse("https://example.com/").unwrap();
        assert_eq!(filename_from_url(&url), None);
    }

    proptest! {
        #[test]
        fn content_disposition_never_panics(
            header in proptest::string::string_regex(
                r#"[A-Za-z0-9;=\\'"%*.\-]{0,512}"#,
            )
            .unwrap(),
        ) {
            if let Some(filename) = filename_from_content_disposition(&header) {
                prop_assert!(!filename.is_empty());
                prop_assert!(!filename.contains('\\'));
                prop_assert!(!filename.contains('/'));
                prop_assert!(!filename.contains(':'));
            }
        }
    }
}
