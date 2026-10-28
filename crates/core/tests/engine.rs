use std::path::Path;
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::extract::Path as AxumPath;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures_util::stream;
use hyperfetch_core::{DownloadId, DownloadSpec, DownloadState, Engine, Settings};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::net::TcpListener;

const CHUNK: u64 = 64 * 1024;

fn byte_at(offset: u64) -> u8 {
    let mixed = offset.wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17) ^ (offset >> 7);
    (mixed >> 11) as u8
}

fn payload(size: u64) -> Vec<u8> {
    (0..size).map(byte_at).collect()
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
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
        let chunk: Vec<u8> = (0..length as u64)
            .map(|index| byte_at(position + index))
            .collect();
        Some((
            Ok::<Bytes, std::io::Error>(Bytes::from(chunk)),
            position + length as u64,
        ))
    });
    Body::from_stream(stream)
}

fn parse_range(value: &str, size: u64) -> Option<(u64, u64)> {
    let value = value.strip_prefix("bytes=")?;
    let (start, end) = value.split_once('-')?;
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

async fn file(AxumPath(size): AxumPath<u64>, headers: HeaderMap) -> Response {
    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| parse_range(value, size));

    match range {
        Some((start, end)) => {
            let length = end - start + 1;
            Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"))
                .header(header::CONTENT_LENGTH, length)
                .body(body_stream(start, end + 1, None))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::OK)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_LENGTH, size)
            .body(body_stream(0, size, None))
            .unwrap(),
    }
}

async fn slow(AxumPath(size): AxumPath<u64>, headers: HeaderMap) -> Response {
    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| parse_range(value, size));

    match range {
        Some((start, end)) => Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"))
            .header(header::CONTENT_LENGTH, end - start + 1)
            .body(body_stream(start, end + 1, Some(Duration::from_millis(25))))
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::OK)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_LENGTH, size)
            .body(body_stream(0, size, Some(Duration::from_millis(25))))
            .unwrap(),
    }
}

async fn norange(AxumPath(size): AxumPath<u64>) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_LENGTH, size)
        .body(body_stream(0, size, None))
        .unwrap()
}

async fn start_server() -> String {
    let app = Router::new()
        .route("/file/{size}", get(file))
        .route("/slow/{size}", get(slow))
        .route("/norange/{size}", get(norange));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{address}")
}

fn test_settings(dir: &Path, auto_resume: bool) -> Settings {
    Settings {
        download_dir: dir.to_string_lossy().into_owned(),
        auto_resume,
        categorize: false,
        max_connections: 16,
        compute_sha256: true,
        ..Settings::default()
    }
}

async fn wait_for<F>(
    engine: &Engine,
    id: DownloadId,
    mut predicate: F,
) -> hyperfetch_core::DownloadSnapshot
where
    F: FnMut(&hyperfetch_core::DownloadSnapshot) -> bool,
{
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let snapshot = engine.get(id).await.expect("download exists");
        if predicate(&snapshot) {
            return snapshot;
        }
        if Instant::now() > deadline {
            panic!("timed out waiting for download state; last: {snapshot:?}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn segmented_download_is_bit_exact_and_accelerated() {
    let server = start_server().await;
    let temp = TempDir::new().unwrap();
    let engine = Engine::open(
        temp.path().join("db.sqlite"),
        Some(test_settings(temp.path(), false)),
    )
    .await
    .unwrap();

    let size = 5 * 1024 * 1024u64;
    let spec = DownloadSpec {
        url: format!("{server}/file/{size}"),
        dir: Some(temp.path().to_string_lossy().into_owned()),
        filename: Some("payload.bin".to_string()),
        connections: Some(8),
        start_now: true,
        ..Default::default()
    };

    let snapshot = engine.add(spec).await.unwrap();
    assert_eq!(snapshot.filename, "payload.bin");
    let done = wait_for(&engine, snapshot.id, |snapshot| {
        snapshot.state == DownloadState::Done
    })
    .await;

    assert_eq!(done.received, size);
    assert_eq!(done.filename, "payload.bin");
    let path = Path::new(&done.dir).join(&done.filename);
    let data = tokio::fs::read(&path).await.unwrap();
    assert_eq!(data.len() as u64, size);
    assert_eq!(sha256(&data), sha256(&payload(size)));
    assert_eq!(
        done.sha256.as_deref(),
        Some(sha256(&payload(size)).as_str())
    );

    let ranges = engine
        .list()
        .await
        .unwrap()
        .into_iter()
        .find(|download| download.id == snapshot.id)
        .unwrap();
    assert_eq!(ranges.state, DownloadState::Done);

    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn server_without_range_support_falls_back_to_single_stream() {
    let server = start_server().await;
    let temp = TempDir::new().unwrap();
    let engine = Engine::open(
        temp.path().join("db.sqlite"),
        Some(test_settings(temp.path(), false)),
    )
    .await
    .unwrap();

    let size = 6 * 1024 * 1024u64;
    let spec = DownloadSpec {
        url: format!("{server}/norange/{size}"),
        dir: Some(temp.path().to_string_lossy().into_owned()),
        connections: Some(8),
        start_now: true,
        ..Default::default()
    };

    let snapshot = engine.add(spec).await.unwrap();
    let done = wait_for(&engine, snapshot.id, |snapshot| {
        snapshot.state == DownloadState::Done
    })
    .await;

    assert_eq!(done.received, size);
    let path = Path::new(&done.dir).join(&done.filename);
    let data = tokio::fs::read(&path).await.unwrap();
    assert_eq!(sha256(&data), sha256(&payload(size)));

    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_and_resume_completes_without_corruption() {
    let server = start_server().await;
    let temp = TempDir::new().unwrap();
    let engine = Engine::open(
        temp.path().join("db.sqlite"),
        Some(test_settings(temp.path(), false)),
    )
    .await
    .unwrap();

    let size = 8 * 1024 * 1024u64;
    let spec = DownloadSpec {
        url: format!("{server}/slow/{size}"),
        dir: Some(temp.path().to_string_lossy().into_owned()),
        connections: Some(4),
        start_now: true,
        ..Default::default()
    };

    let snapshot = engine.add(spec).await.unwrap();
    wait_for(&engine, snapshot.id, |snapshot| snapshot.received > 400_000).await;

    engine.pause(snapshot.id).await.unwrap();
    let paused = wait_for(&engine, snapshot.id, |snapshot| {
        snapshot.state == DownloadState::Paused
    })
    .await;
    assert!(paused.received < size);

    // Give any straggling worker a chance to misbehave, then ensure we are stable.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let still_paused = engine.get(snapshot.id).await.unwrap();
    assert_eq!(still_paused.state, DownloadState::Paused);

    engine.resume(snapshot.id).await.unwrap();
    let done = wait_for(&engine, snapshot.id, |snapshot| {
        snapshot.state == DownloadState::Done
    })
    .await;

    let path = Path::new(&done.dir).join(&done.filename);
    let data = tokio::fs::read(&path).await.unwrap();
    assert_eq!(data.len() as u64, size);
    assert_eq!(sha256(&data), sha256(&payload(size)));

    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn crash_recovery_resumes_automatically() {
    let server = start_server().await;
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("db.sqlite");

    let size = 8 * 1024 * 1024u64;
    let id;
    {
        let engine = Engine::open(&db_path, Some(test_settings(temp.path(), true)))
            .await
            .unwrap();

        let spec = DownloadSpec {
            url: format!("{server}/slow/{size}"),
            dir: Some(temp.path().to_string_lossy().into_owned()),
            connections: Some(4),
            start_now: true,
            ..Default::default()
        };

        let snapshot = engine.add(spec).await.unwrap();
        id = snapshot.id;
        wait_for(&engine, id, |snapshot| snapshot.received > 300_000).await;

        engine.shutdown().await;
    }

    // Reopen the database as a fresh application launch would.
    let engine = Engine::open(&db_path, None).await.unwrap();
    let done = wait_for(&engine, id, |snapshot| {
        snapshot.state == DownloadState::Done
    })
    .await;

    let path = Path::new(&done.dir).join(&done.filename);
    let data = tokio::fs::read(&path).await.unwrap();
    assert_eq!(data.len() as u64, size);
    assert_eq!(sha256(&data), sha256(&payload(size)));

    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queue_runs_downloads_and_survives_concurrency_limit() {
    let server = start_server().await;
    let temp = TempDir::new().unwrap();
    let mut settings = test_settings(temp.path(), false);
    settings.max_concurrent = 2;

    let engine = Engine::open(temp.path().join("db.sqlite"), Some(settings))
        .await
        .unwrap();

    let mut ids = Vec::new();
    for index in 0..4u64 {
        let size = (2 + index) * 1024 * 1024;
        let spec = DownloadSpec {
            url: format!("{server}/file/{size}"),
            dir: Some(
                temp.path()
                    .join(format!("batch-{index}"))
                    .to_string_lossy()
                    .into_owned(),
            ),
            connections: Some(4),
            start_now: true,
            ..Default::default()
        };
        let snapshot = engine.add(spec).await.unwrap();
        ids.push((snapshot.id, size));
    }

    for (id, size) in ids {
        let done = wait_for(&engine, id, |snapshot| {
            snapshot.state == DownloadState::Done
        })
        .await;
        assert_eq!(done.received, size);
        let data = tokio::fs::read(Path::new(&done.dir).join(&done.filename))
            .await
            .unwrap();
        assert_eq!(sha256(&data), sha256(&payload(size)));
    }

    engine.shutdown().await;
}
