use std::collections::HashMap;
use std::time::Duration;

use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{Error, Result};
use crate::util::{filename_from_content_disposition, filename_from_url};

/// Everything learned about a remote resource before the transfer starts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub final_url: String,
    pub filename: String,
    pub total_size: u64,
    pub accept_ranges: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_type: Option<String>,
    pub content_disposition: Option<String>,
}

pub fn apply_headers(
    mut request: RequestBuilder,
    headers: &HashMap<String, String>,
) -> RequestBuilder {
    for (name, value) in headers {
        request = request.header(name, value);
    }
    request
}

/// Convenience wrapper that builds a standalone client (used by the CLI and tests).
pub async fn probe_url(url: &Url) -> Result<ProbeResult> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent(format!("HyperFetch/{}", env!("CARGO_PKG_VERSION")))
        .build()?;
    probe_with_client(&client, url, &HashMap::new()).await
}

pub async fn probe_with_client(
    client: &Client,
    url: &Url,
    headers: &HashMap<String, String>,
) -> Result<ProbeResult> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::InvalidInput(format!(
            "unsupported URL scheme '{}' (only http and https are supported)",
            url.scheme()
        )));
    }

    let head = apply_headers(client.head(url.clone()), headers)
        .timeout(Duration::from_secs(45))
        .send()
        .await;

    let head_result = match head {
        Ok(response) if response.status().is_success() => Some(response),
        _ => None,
    };

    let need_get_probe = match &head_result {
        Some(response) => content_length(response).is_none() || !accepts_ranges(response),
        None => true,
    };

    if need_get_probe {
        match probe_with_range(client, url, headers).await {
            Ok(probe) => return Ok(probe),
            Err(error) => {
                if let Some(response) = head_result {
                    return Ok(from_response(
                        &response,
                        url,
                        None,
                        accepts_ranges(&response),
                    ));
                }
                return Err(error);
            }
        }
    }

    let response = head_result.expect("checked above");
    let total = content_length(&response);
    let ranges = accepts_ranges(&response);
    Ok(from_response(&response, url, total, ranges))
}

async fn probe_with_range(
    client: &Client,
    url: &Url,
    headers: &HashMap<String, String>,
) -> Result<ProbeResult> {
    let response = apply_headers(client.get(url.clone()), headers)
        .header(reqwest::header::RANGE, "bytes=0-0")
        .timeout(Duration::from_secs(45))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(Error::failed(format!(
            "server responded with HTTP {}",
            response.status()
        )));
    }

    let partial = response.status() == StatusCode::PARTIAL_CONTENT;
    let total = if partial {
        content_range_total(&response)
    } else {
        content_length(&response)
    };
    let ranges = partial || accepts_ranges(&response);
    Ok(from_response(&response, url, total, ranges))
}

fn from_response(
    response: &Response,
    requested_url: &Url,
    total_size: Option<u64>,
    accept_ranges: bool,
) -> ProbeResult {
    let final_url = response.url().clone();
    let filename = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .and_then(filename_from_content_disposition)
        .or_else(|| filename_from_url(&final_url))
        .or_else(|| filename_from_url(requested_url))
        .unwrap_or_else(|| "download".to_string());

    ProbeResult {
        final_url: final_url.to_string(),
        filename,
        total_size: total_size.unwrap_or(0),
        accept_ranges,
        etag: header_string(response, reqwest::header::ETAG),
        last_modified: header_string(response, reqwest::header::LAST_MODIFIED),
        content_type: header_string(response, reqwest::header::CONTENT_TYPE),
        content_disposition: header_string(response, reqwest::header::CONTENT_DISPOSITION),
    }
}

fn header_string(response: &Response, header: reqwest::header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(header)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

fn content_length(response: &Response) -> Option<u64> {
    header_string(response, reqwest::header::CONTENT_LENGTH)?
        .parse()
        .ok()
}

fn accepts_ranges(response: &Response) -> bool {
    response
        .headers()
        .get(reqwest::header::ACCEPT_RANGES)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase().contains("bytes"))
        .unwrap_or(false)
}

/// Parse `bytes 0-0/1234` (or `bytes 0-0/*`) into the total size.
fn content_range_total(response: &Response) -> Option<u64> {
    let value = header_string(response, reqwest::header::CONTENT_RANGE)?;
    let total = value.rsplit('/').next()?.trim();
    if total == "*" {
        None
    } else {
        total.parse().ok()
    }
}
