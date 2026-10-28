//! Recursive site grabber.
//!
//! A polite, bounded crawler: it respects a page/depth/domain budget and can
//! optionally honour `robots.txt` through the caller's policy.

use std::collections::{HashSet, VecDeque};

use regex::Regex;
use url::Url;

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct CrawlOptions {
    pub max_depth: u32,
    pub max_pages: usize,
    pub same_host_only: bool,
    /// Only follow links whose path extension matches one of these values.
    /// Empty means "follow every discovered document link".
    pub extensions: Vec<String>,
}

impl Default for CrawlOptions {
    fn default() -> Self {
        Self {
            max_depth: 2,
            max_pages: 200,
            same_host_only: true,
            extensions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DiscoveredLink {
    pub url: String,
    pub depth: u32,
    pub is_file: bool,
}

/// Extract `href` and `src` targets from an HTML document.
pub fn extract_links(base: &Url, html: &str) -> Vec<Url> {
    let pattern = Regex::new(r#"(?i)(?:href|src)\s*=\s*["']([^"'#]+)["']"#).expect("valid regex");
    let mut seen = HashSet::new();
    let mut links = Vec::new();

    for capture in pattern.captures_iter(html) {
        let raw = capture.get(1).map(|value| value.as_str()).unwrap_or("");
        if raw.is_empty() || raw.starts_with("javascript:") || raw.starts_with("mailto:") {
            continue;
        }
        if let Ok(url) = base.join(raw) {
            if matches!(url.scheme(), "http" | "https") && seen.insert(url.to_string()) {
                links.push(url);
            }
        }
    }

    links
}

/// Resolve a URL's target filename extension, if any.
pub fn is_probable_file(url: &Url) -> bool {
    let path = url.path();
    let extension = path.rsplit('.').next().unwrap_or("");
    !path.ends_with('/')
        && !extension.is_empty()
        && extension.len() <= 5
        && extension
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
        && !matches!(extension, "html" | "htm" | "php" | "asp" | "aspx" | "jsp")
}

/// Walk a site breadth-first. The callback receives links as they are found and
/// returns the HTML for pages it wants to have crawled (return `None` to stop
/// that branch).
pub async fn crawl<F, Fut>(
    start: Url,
    options: CrawlOptions,
    mut fetch: F,
) -> Result<Vec<DiscoveredLink>>
where
    F: FnMut(Url) -> Fut,
    Fut: std::future::Future<Output = Result<Option<String>>>,
{
    if !matches!(start.scheme(), "http" | "https") {
        return Err(Error::InvalidInput("crawl URL must be http(s)".into()));
    }

    let host = start.host_str().map(str::to_string);
    let mut queue = VecDeque::from([(start.clone(), 0u32)]);
    let mut visited: HashSet<String> = HashSet::new();
    let mut discovered = Vec::new();

    while let Some((url, depth)) = queue.pop_front() {
        if visited.len() >= options.max_pages {
            break;
        }
        if !visited.insert(url.to_string()) {
            continue;
        }
        if options.same_host_only && url.host_str() != host.as_deref() {
            continue;
        }
        if is_probable_file(&url) {
            discovered.push(DiscoveredLink {
                url: url.to_string(),
                depth,
                is_file: true,
            });
            continue;
        }
        if depth >= options.max_depth {
            continue;
        }

        let html = match fetch(url.clone()).await? {
            Some(html) => html,
            None => continue,
        };

        for link in extract_links(&url, &html) {
            if options.same_host_only && link.host_str() != host.as_deref() {
                continue;
            }
            if is_probable_file(&link) {
                discovered.push(DiscoveredLink {
                    url: link.to_string(),
                    depth: depth + 1,
                    is_file: true,
                });
                continue;
            }
            queue.push_back((link, depth + 1));
        }
    }

    Ok(discovered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_absolute_and_relative_links() {
        let base = Url::parse("https://example.com/dir/page.html").unwrap();
        let html = r##"
            <a href="/one.html">one</a>
            <a href='two.zip'>two</a>
            <img src="https://cdn.example.com/image.png">
            <a href="javascript:void(0)">no</a>
            <a href="#anchor">no</a>
        "##;
        let links = extract_links(&base, html);
        let urls: Vec<String> = links.iter().map(Url::to_string).collect();
        assert!(urls.contains(&"https://example.com/one.html".to_string()));
        assert!(urls.contains(&"https://example.com/dir/two.zip".to_string()));
        assert!(urls.contains(&"https://cdn.example.com/image.png".to_string()));
        assert_eq!(urls.len(), 3);
    }

    #[test]
    fn detects_files() {
        assert!(is_probable_file(
            &Url::parse("https://x.test/a.zip").unwrap()
        ));
        assert!(!is_probable_file(
            &Url::parse("https://x.test/a.html").unwrap()
        ));
        assert!(!is_probable_file(
            &Url::parse("https://x.test/dir/").unwrap()
        ));
        assert!(!is_probable_file(&Url::parse("https://x.test/").unwrap()));
    }
}
