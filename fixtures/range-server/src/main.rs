use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use clap::Parser;
use futures_util::stream;
use tokio::net::TcpListener;

const CHUNK: u64 = 64 * 1024;
static ZERO_CHUNK: [u8; CHUNK as usize] = [0u8; CHUNK as usize];

#[derive(Default)]
struct FixtureStats {
    requests: AtomicU64,
    head_requests: AtomicU64,
    range_requests: AtomicU64,
    bytes: AtomicU64,
}

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 8080)]
    port: u16,
}

/// Deterministic pseudo-random payload used by tests and manual runs.
pub fn byte_at(offset: u64) -> u8 {
    let mixed = offset.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17) ^ (offset >> 7);
    (mixed >> 11) as u8
}

fn bytes_at(offset: u64, len: usize) -> Vec<u8> {
    (0..len as u64)
        .map(|index| byte_at(offset + index))
        .collect()
}

fn body_stream(start: u64, end: u64, delay: Option<Duration>) -> Body {
    let stream = stream::unfold(start, move |position| async move {
        if position >= end {
            return None;
        }
        if let Some(delay) = delay {
            tokio::time::sleep(delay).await;
        }
        let length = (end - position).min(CHUNK) as usize;
        let chunk = Bytes::from(bytes_at(position, length));
        Some((Ok::<Bytes, std::io::Error>(chunk), position + length as u64))
    });
    Body::from_stream(stream)
}

fn zero_stream(start: u64, end: u64) -> Body {
    let stream = stream::unfold(start, move |position| async move {
        if position >= end {
            return None;
        }
        let length = (end - position).min(CHUNK) as usize;
        let chunk = Bytes::from_static(&ZERO_CHUNK[..length]);
        Some((Ok::<Bytes, std::io::Error>(chunk), position + length as u64))
    });
    Body::from_stream(stream)
}

fn parse_range(value: &str, size: u64) -> Option<(u64, u64)> {
    let value = value.strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;

    if start.is_empty() {
        let suffix: u64 = end.parse().ok()?;
        if suffix == 0 {
            return None;
        }
        let start = size.saturating_sub(suffix);
        return Some((start, size.saturating_sub(1)));
    }

    let start: u64 = start.parse().ok()?;
    let end: u64 = if end.is_empty() {
        size.saturating_sub(1)
    } else {
        end.parse().ok()?
    };

    if start > end || start >= size {
        return None;
    }
    Some((start, end.min(size.saturating_sub(1))))
}

async fn ranged(size: u64, headers: &HeaderMap, delay: Option<Duration>) -> Response {
    if size == 0 {
        return StatusCode::OK.into_response();
    }

    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| parse_range(value, size));

    match range {
        Some((start, end)) => {
            let length = end - start + 1;
            Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"))
                .header(header::CONTENT_LENGTH, length)
                .body(body_stream(start, end + 1, delay))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_LENGTH, size)
            .body(body_stream(0, size, delay))
            .unwrap(),
    }
}

async fn handler_file(Path(size): Path<u64>, headers: HeaderMap) -> Response {
    ranged(size, &headers, None).await
}

async fn handler_norange(Path(size): Path<u64>) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, size)
        .body(body_stream(0, size, None))
        .unwrap()
}

async fn handler_slow(Path(size): Path<u64>, headers: HeaderMap) -> Response {
    ranged(size, &headers, Some(Duration::from_millis(20))).await
}

async fn handler_drop(Path(size): Path<u64>) -> Response {
    // Advertise the full size but stop halfway; the client must retry.
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, size)
        .body(body_stream(0, size / 2, None))
        .unwrap()
}

async fn handler_etag(Path(size): Path<u64>, headers: HeaderMap) -> Response {
    let mut response = ranged(size, &headers, None).await;
    response
        .headers_mut()
        .insert(header::ETAG, "\"fixture-v1\"".parse().unwrap());
    response
}

/// Zero-filled, allocation-free range endpoint used for throughput benchmarks.
async fn handler_stream(
    State(stats): State<Arc<FixtureStats>>,
    method: Method,
    Path(size): Path<u64>,
    headers: HeaderMap,
) -> Response {
    stats.requests.fetch_add(1, Ordering::Relaxed);
    let is_head = method == Method::HEAD;
    if is_head {
        stats.head_requests.fetch_add(1, Ordering::Relaxed);
    }
    if headers.contains_key(header::RANGE) {
        stats.range_requests.fetch_add(1, Ordering::Relaxed);
    }

    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| parse_range(value, size));

    match range {
        Some((start, end)) => {
            let length = end - start + 1;
            if !is_head {
                stats.bytes.fetch_add(length, Ordering::Relaxed);
            }
            Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"))
                .header(header::CONTENT_LENGTH, length)
                .body(zero_stream(start, end + 1))
                .unwrap()
        }
        None => {
            if !is_head {
                stats.bytes.fetch_add(size, Ordering::Relaxed);
            }
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/octet-stream")
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, size)
                .body(zero_stream(0, size))
                .unwrap()
        }
    }
}

async fn stats(State(stats): State<Arc<FixtureStats>>) -> Response {
    let body = format!(
        "{{\"requests\":{},\"head_requests\":{},\"range_requests\":{},\"bytes\":{}}}",
        stats.requests.load(Ordering::Relaxed),
        stats.head_requests.load(Ordering::Relaxed),
        stats.range_requests.load(Ordering::Relaxed),
        stats.bytes.load(Ordering::Relaxed)
    );
    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

async fn reset_stats(State(stats): State<Arc<FixtureStats>>) -> &'static str {
    stats.requests.store(0, Ordering::Relaxed);
    stats.head_requests.store(0, Ordering::Relaxed);
    stats.range_requests.store(0, Ordering::Relaxed);
    stats.bytes.store(0, Ordering::Relaxed);
    "ok"
}

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().init();
    let args = Args::parse();

    let app = Router::new()
        .route("/health", get(health))
        .route("/stats", get(stats))
        .route("/stats/reset", get(reset_stats))
        .route("/file/{size}", get(handler_file))
        .route("/stream/{size}", get(handler_stream))
        .route("/norange/{size}", get(handler_norange))
        .route("/slow/{size}", get(handler_slow))
        .route("/drop/{size}", get(handler_drop))
        .route("/etag/{size}", get(handler_etag))
        .with_state(Arc::new(FixtureStats::default()));

    let address = SocketAddr::from(([127, 0, 0, 1], args.port));
    let listener = TcpListener::bind(address).await?;
    tracing::info!("fixture server listening on http://{address}");
    axum::serve(listener, app).await?;
    Ok(())
}
