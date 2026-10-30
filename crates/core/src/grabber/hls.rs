//! HLS manifest inspection and downloading.
//!
//! HyperFetch downloads unencrypted media and standard AES-128 HLS segments
//! whose key URI is published in the playlist. Widevine, PlayReady, FairPlay
//! and other DRM systems are explicitly out of scope.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockDecryptMut, KeyIvInit};
use futures_util::StreamExt;
use reqwest::Client;
use tokio::io::AsyncWriteExt;
use url::Url;

use crate::error::{Error, Result};

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// A media rendition advertised by an HLS master playlist.
#[derive(Debug, Clone)]
pub struct Variant {
    pub uri: String,
    pub bandwidth: Option<u64>,
    pub resolution: Option<String>,
}

/// Key information for encrypted segments.
#[derive(Debug, Clone, PartialEq)]
pub struct HlsKey {
    pub method: String,
    pub uri: Option<String>,
    pub iv: Option<Vec<u8>>,
}

/// Initialization section (`EXT-X-MAP`) used by fragmented-MP4 playlists.
#[derive(Debug, Clone, PartialEq)]
pub struct HlsMap {
    pub uri: String,
}

#[derive(Debug, Clone)]
pub struct HlsSegment {
    pub uri: String,
    pub duration: Option<f64>,
    pub key: Option<HlsKey>,
    pub map: Option<HlsMap>,
}

/// Parsed HLS playlist.
#[derive(Debug, Clone)]
pub struct HlsPlaylist {
    pub variants: Vec<Variant>,
    pub segments: Vec<HlsSegment>,
    pub is_master: bool,
    pub media_sequence: u64,
}

impl HlsPlaylist {
    pub fn parse(content: &str) -> Result<Self> {
        if !content.trim_start().starts_with("#EXTM3U") {
            return Err(Error::InvalidInput("not an HLS playlist".into()));
        }

        let mut variants = Vec::new();
        let mut segments = Vec::new();
        let mut pending_bandwidth: Option<u64> = None;
        let mut pending_resolution: Option<String> = None;
        let mut pending_duration: Option<f64> = None;
        let mut current_key: Option<HlsKey> = None;
        let mut current_map: Option<HlsMap> = None;
        let mut media_sequence = 0u64;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
                let attributes = parse_attributes(rest);
                pending_bandwidth = attributes
                    .get("BANDWIDTH")
                    .and_then(|value| value.parse().ok());
                pending_resolution = attributes.get("RESOLUTION").cloned();
            } else if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
                media_sequence = rest.trim().parse().unwrap_or(0);
            } else if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
                let attributes = parse_attributes(rest);
                if let Some(uri) = attributes.get("URI") {
                    current_map = Some(HlsMap { uri: uri.clone() });
                }
            } else if let Some(rest) = line.strip_prefix("#EXT-X-KEY:") {
                let attributes = parse_attributes(rest);
                let method = attributes
                    .get("METHOD")
                    .cloned()
                    .unwrap_or_else(|| "NONE".to_string());
                if method.eq_ignore_ascii_case("NONE") {
                    current_key = None;
                } else {
                    current_key = Some(HlsKey {
                        method,
                        uri: attributes.get("URI").cloned(),
                        iv: attributes.get("IV").and_then(|value| parse_iv(value)),
                    });
                }
            } else if let Some(rest) = line.strip_prefix("#EXTINF:") {
                let duration = rest.split(',').next().unwrap_or("").trim();
                pending_duration = duration.parse().ok();
            } else if !line.starts_with('#') {
                if pending_bandwidth.is_some() || pending_resolution.is_some() {
                    variants.push(Variant {
                        uri: line.to_string(),
                        bandwidth: pending_bandwidth.take(),
                        resolution: pending_resolution.take(),
                    });
                } else {
                    segments.push(HlsSegment {
                        uri: line.to_string(),
                        duration: pending_duration.take(),
                        key: current_key.clone(),
                        map: current_map.clone(),
                    });
                }
            }
        }

        Ok(Self {
            is_master: !variants.is_empty(),
            variants,
            segments,
            media_sequence,
        })
    }

    /// Pick the highest-bandwidth variant from a master playlist.
    pub fn best_variant(&self) -> Option<&Variant> {
        self.variants
            .iter()
            .max_by_key(|variant| variant.bandwidth.unwrap_or(0))
    }
}

fn parse_attributes(input: &str) -> HashMap<String, String> {
    let mut attributes = HashMap::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut parts = Vec::new();

    for character in input.chars() {
        match character {
            '"' => {
                in_quotes = !in_quotes;
                current.push(character);
            }
            ',' if !in_quotes => {
                parts.push(current.clone());
                current.clear();
            }
            _ => current.push(character),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }

    for part in parts {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_string();
        attributes.insert(key.trim().to_string(), value);
    }

    attributes
}

fn parse_iv(value: &str) -> Option<Vec<u8>> {
    let value = value
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    if value.len() != 32 {
        return None;
    }
    let mut bytes = Vec::with_capacity(16);
    for index in (0..32).step_by(2) {
        let byte = u8::from_str_radix(&value[index..index + 2], 16).ok()?;
        bytes.push(byte);
    }
    Some(bytes)
}

#[derive(Debug, Clone)]
pub struct HlsSummary {
    pub bytes: u64,
    pub segments: u64,
    pub output: PathBuf,
}

pub type ProgressCallback = Arc<dyn Fn(u64, u64, u64) + Send + Sync>;

/// Download an HLS stream (master or media playlist) into a single file.
pub async fn download_hls(
    client: &Client,
    master_url: &Url,
    output: &Path,
    connections: usize,
    progress: Option<ProgressCallback>,
) -> Result<HlsSummary> {
    let (playlist_url, text) = fetch_text(client, master_url).await?;
    let mut playlist = HlsPlaylist::parse(&text)?;

    let playlist_url = if playlist.is_master {
        let variant = playlist
            .best_variant()
            .ok_or_else(|| Error::failed("HLS master playlist has no variants"))?;
        let variant_url = playlist_url.join(&variant.uri)?;
        let (final_url, text) = fetch_text(client, &variant_url).await?;
        playlist = HlsPlaylist::parse(&text)?;
        final_url
    } else {
        playlist_url
    };

    if playlist.is_master {
        return Err(Error::failed(
            "nested HLS master playlists are not supported",
        ));
    }
    if playlist.segments.is_empty() {
        return Err(Error::failed("HLS playlist contains no segments"));
    }

    // Pre-fetch every key used by the playlist.
    let mut keys: HashMap<String, Vec<u8>> = HashMap::new();
    for segment in &playlist.segments {
        if let Some(key) = &segment.key {
            if key.method.eq_ignore_ascii_case("AES-128") {
                let Some(uri) = &key.uri else {
                    return Err(Error::failed("encrypted segment is missing a key URI"));
                };
                if !keys.contains_key(uri) {
                    let key_url = playlist_url.join(uri)?;
                    let response = client.get(key_url.clone()).send().await?;
                    if !response.status().is_success() {
                        return Err(Error::failed(format!(
                            "cannot fetch HLS key (HTTP {})",
                            response.status()
                        )));
                    }
                    let bytes = response.bytes().await?;
                    if bytes.len() != 16 {
                        return Err(Error::failed("HLS AES-128 key must be 16 bytes"));
                    }
                    keys.insert(uri.clone(), bytes.to_vec());
                }
            } else if !key.method.eq_ignore_ascii_case("NONE") {
                return Err(Error::failed(format!(
                    "unsupported HLS encryption method '{}' (DRM is not supported)",
                    key.method
                )));
            }
        }
    }

    let mut maps: HashMap<String, Vec<u8>> = HashMap::new();
    for segment in &playlist.segments {
        let Some(map) = &segment.map else {
            continue;
        };
        if maps.contains_key(&map.uri) {
            continue;
        }
        let map_url = playlist_url.join(&map.uri)?;
        let response = client.get(map_url).send().await?;
        if !response.status().is_success() {
            return Err(Error::failed(format!(
                "cannot fetch HLS init section (HTTP {})",
                response.status()
            )));
        }
        maps.insert(map.uri.clone(), response.bytes().await?.to_vec());
    }

    let total_segments = playlist.segments.len() as u64;
    let mut written = 0u64;
    let mut completed = 0u64;
    let mut last_map: Option<String> = None;

    if let Some(parent) = output.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(output)
        .await?;

    let window = connections.max(1);
    let keys = Arc::new(keys);
    let sequence = playlist.media_sequence;
    let segments = playlist.segments.clone();

    let mut futures = Vec::with_capacity(segments.len());
    for (index, segment) in segments.iter().enumerate() {
        let url = playlist_url.join(&segment.uri)?;
        let client = client.clone();
        let key = segment.key.clone();
        let keys = Arc::clone(&keys);
        let map_uri = segment.map.as_ref().map(|map| map.uri.clone());
        let absolute_index = index as u64;

        futures.push(async move {
            let response = client.get(url.clone()).send().await?;
            if !response.status().is_success() {
                return Err(Error::failed(format!(
                    "segment request failed with HTTP {}",
                    response.status()
                )));
            }
            let bytes = response.bytes().await?;
            let data = match key {
                Some(key) if key.method.eq_ignore_ascii_case("AES-128") => {
                    let uri = key
                        .uri
                        .as_ref()
                        .ok_or_else(|| Error::failed("missing key URI"))?;
                    let key_bytes = keys
                        .get(uri)
                        .ok_or_else(|| Error::failed("missing HLS key"))?;
                    let iv = match key.iv.as_ref() {
                        Some(iv) if iv.len() == 16 => {
                            let mut value = [0u8; 16];
                            value.copy_from_slice(iv);
                            value
                        }
                        _ => {
                            let mut value = [0u8; 16];
                            let number = sequence + absolute_index;
                            value[8..].copy_from_slice(&number.to_be_bytes());
                            value
                        }
                    };
                    decrypt_segment(&bytes, key_bytes, &iv)?
                }
                _ => bytes.to_vec(),
            };
            Ok::<(Vec<u8>, Option<String>), Error>((data, map_uri))
        });
    }

    let mut stream = futures_util::stream::iter(futures).buffered(window);
    while let Some(result) = stream.next().await {
        let (data, map_uri) = result?;
        if let Some(uri) = &map_uri {
            if last_map.as_deref() != Some(uri.as_str()) {
                let init = maps
                    .get(uri)
                    .ok_or_else(|| Error::failed("missing HLS init section"))?;
                file.write_all(init).await?;
                written += init.len() as u64;
                last_map = Some(uri.clone());
            }
        } else {
            last_map = None;
        }
        file.write_all(&data).await?;
        written += data.len() as u64;
        completed += 1;
        if let Some(callback) = &progress {
            callback(completed, total_segments, written);
        }
    }

    file.flush().await?;
    file.sync_all().await?;

    Ok(HlsSummary {
        bytes: written,
        segments: completed,
        output: output.to_path_buf(),
    })
}

fn decrypt_segment(data: &[u8], key: &[u8], iv: &[u8; 16]) -> Result<Vec<u8>> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    #[cfg(feature = "native-accel")]
    {
        if key.len() == 16 {
            if let Some(decrypted) = hyperfetch_native::decrypt_aes128_cbc_padded(
                key.try_into().expect("checked 16-byte key"),
                iv,
                data,
            ) {
                return Ok(decrypted);
            }
        }
    }
    let mut buffer = data.to_vec();
    let decrypted = Aes128CbcDec::new_from_slices(key, iv)
        .map_err(|error| Error::failed(format!("invalid AES key/IV: {error}")))?
        .decrypt_padded_mut::<Pkcs7>(&mut buffer)
        .map_err(|_| Error::failed("AES-128 padding error (wrong key?)"))?;
    Ok(decrypted.to_vec())
}

async fn fetch_text(client: &Client, url: &Url) -> Result<(Url, String)> {
    let response = client.get(url.clone()).send().await?;
    if !response.status().is_success() {
        return Err(Error::failed(format!(
            "playlist request failed with HTTP {}",
            response.status()
        )));
    }
    let final_url = response.url().clone();
    let text = response.text().await?;
    Ok((final_url, text))
}

/// Quick check used by the browser bridge.
pub fn looks_like_hls(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains(".m3u8")
}

/// Quick check used by the browser bridge.
pub fn looks_like_dash(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains(".mpd")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn parses_master_playlist() {
        let playlist = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=800000,RESOLUTION=640x360
low/index.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=2400000,RESOLUTION=1280x720
high/index.m3u8
"#;
        let parsed = HlsPlaylist::parse(playlist).unwrap();
        assert!(parsed.is_master);
        assert_eq!(parsed.variants.len(), 2);
        assert_eq!(parsed.variants[1].resolution.as_deref(), Some("1280x720"));
        assert_eq!(parsed.best_variant().unwrap().bandwidth, Some(2_400_000));
    }

    #[test]
    fn parses_media_playlist_with_keys() {
        let playlist = r#"#EXTM3U
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:42
#EXT-X-KEY:METHOD=AES-128,URI="key.bin",IV=0x00000000000000000000000000000001
#EXTINF:9.9,
segment0.ts
#EXT-X-KEY:METHOD=NONE
#EXTINF:9.9,
segment1.ts
#EXT-X-ENDLIST
"#;
        let parsed = HlsPlaylist::parse(playlist).unwrap();
        assert!(!parsed.is_master);
        assert_eq!(parsed.segments.len(), 2);
        assert_eq!(parsed.media_sequence, 42);
        assert!(parsed.segments[0].key.is_some());
        assert!(parsed.segments[1].key.is_none());
        assert_eq!(
            parsed.segments[0]
                .key
                .as_ref()
                .unwrap()
                .iv
                .as_ref()
                .unwrap()[15],
            1
        );
    }

    #[test]
    fn parses_fragmented_mp4_init_sections() {
        let playlist = r#"#EXTM3U
#EXT-X-TARGETDURATION:6
#EXT-X-MAP:URI="init.mp4"
#EXTINF:6.0,
segment0.m4s
#EXTINF:6.0,
segment1.m4s
#EXT-X-ENDLIST
"#;
        let parsed = HlsPlaylist::parse(playlist).unwrap();
        assert_eq!(parsed.segments.len(), 2);
        assert_eq!(
            parsed.segments[0].map.as_ref().map(|map| map.uri.as_str()),
            Some("init.mp4")
        );
        assert_eq!(parsed.segments[1].map, parsed.segments[0].map);
    }

    #[test]
    fn decrypts_aes128_padded_segments() {
        use cbc::cipher::{BlockEncryptMut, KeyIvInit};

        type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

        let key = [7u8; 16];
        let iv = [9u8; 16];
        let plaintext = b"encrypted hls segment payload";
        let mut buffer = vec![0u8; plaintext.len() + 16];
        buffer[..plaintext.len()].copy_from_slice(plaintext);
        let length = Aes128CbcEnc::new_from_slices(&key, &iv)
            .unwrap()
            .encrypt_padded_mut::<Pkcs7>(&mut buffer, plaintext.len())
            .unwrap()
            .len();

        let decrypted = decrypt_segment(&buffer[..length], &key, &iv).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn rejects_non_hls() {
        assert!(HlsPlaylist::parse("<html></html>").is_err());
    }

    proptest! {
        #[test]
        fn parsing_arbitrary_input_never_panics(
            prefix_len in 0u8..8,
            body in proptest::string::string_regex(".{0,4096}").unwrap(),
        ) {
            let header = "#EXTM3U\n".repeat(prefix_len.max(1) as usize);
            let content = format!("{header}{body}");
            let parsed = HlsPlaylist::parse(&content);
            let _ = parsed.map(|playlist| {
                let _ = playlist.best_variant();
            });
        }
    }

    proptest! {
        #[test]
        fn attribute_and_iv_parsing_never_panic(
            input in proptest::string::string_regex("[A-Za-z0-9=\"x0-9,]{0,256}").unwrap(),
        ) {
            let attributes = parse_attributes(&input);
            for key in attributes.keys() {
                let _ = attributes.get(key);
            }
            for (key, value) in &attributes {
                if key == "IV" {
                    let _ = parse_iv(value);
                }
            }
        }
    }
}
