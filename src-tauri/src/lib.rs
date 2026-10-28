mod clipboard;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::{HeaderMap, Method, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use clipboard::{ClipboardOffer, ClipboardState};
use hyperfetch_core::grabber::hls::{
    download_hls, looks_like_dash, looks_like_hls, ProgressCallback,
};
use hyperfetch_core::grabber::site::{self, CrawlOptions, DiscoveredLink};
use hyperfetch_core::util::{
    filename_from_url, now_ms, resolve_download_dir, sanitize_filename, unique_destination,
};
use hyperfetch_core::{DownloadSnapshot, DownloadSpec, Engine, EngineStats, ProbeResult, Settings};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use url::Url;
use uuid::Uuid;

#[derive(Clone, Serialize)]
struct BridgeInfo {
    url: String,
    port: u16,
    token: String,
}

struct AppState {
    engine: Engine,
    bridge: BridgeInfo,
    offers: Arc<Mutex<HashMap<Uuid, CaptureOffer>>>,
    clipboard: Arc<ClipboardState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CaptureRequest {
    url: String,
    #[serde(default)]
    filename: Option<String>,
    #[serde(default)]
    referer: Option<String>,
    #[serde(default)]
    cookie: Option<String>,
    #[serde(default)]
    user_agent: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    page_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct CaptureOffer {
    id: Uuid,
    url: String,
    filename: Option<String>,
    kind: String,
    referer: Option<String>,
    cookie: Option<String>,
    user_agent: Option<String>,
    page_url: Option<String>,
    created_at: i64,
}

#[derive(Clone)]
struct BridgeContext {
    engine: Engine,
    token: Arc<String>,
    offers: Arc<Mutex<HashMap<Uuid, CaptureOffer>>>,
    app: AppHandle,
}

#[derive(Debug, Serialize)]
struct CaptureAction {
    action: String,
    id: String,
    message: Option<String>,
}

fn to_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn parse_id(id: &str) -> Result<Uuid, String> {
    Uuid::parse_str(id).map_err(|error| format!("invalid id: {error}"))
}

#[tauri::command]
async fn probe_url(
    state: State<'_, AppState>,
    url: String,
    headers: Option<HashMap<String, String>>,
) -> Result<ProbeResult, String> {
    state
        .engine
        .probe(&url, headers, None)
        .await
        .map_err(to_error)
}

#[tauri::command]
async fn add_download(
    state: State<'_, AppState>,
    spec: DownloadSpec,
) -> Result<DownloadSnapshot, String> {
    state.engine.add(spec).await.map_err(to_error)
}

#[tauri::command]
async fn list_downloads(state: State<'_, AppState>) -> Result<Vec<DownloadSnapshot>, String> {
    state.engine.list().await.map_err(to_error)
}

#[tauri::command]
async fn get_download(state: State<'_, AppState>, id: String) -> Result<DownloadSnapshot, String> {
    state.engine.get(parse_id(&id)?).await.map_err(to_error)
}

#[tauri::command]
async fn pause_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.engine.pause(parse_id(&id)?).await.map_err(to_error)
}

#[tauri::command]
async fn resume_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.engine.resume(parse_id(&id)?).await.map_err(to_error)
}

#[tauri::command]
async fn cancel_download(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.engine.cancel(parse_id(&id)?).await.map_err(to_error)
}

#[tauri::command]
async fn remove_download(
    state: State<'_, AppState>,
    id: String,
    delete_file: bool,
) -> Result<(), String> {
    state
        .engine
        .remove(parse_id(&id)?, delete_file)
        .await
        .map_err(to_error)
}

#[tauri::command]
async fn pause_all(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.pause_all().await.map_err(to_error)
}

#[tauri::command]
async fn resume_all(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.resume_all().await.map_err(to_error)
}

#[tauri::command]
async fn clear_completed(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.clear_completed().await.map_err(to_error)
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.engine.settings().await)
}

#[tauri::command]
async fn update_settings(
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    state
        .engine
        .update_settings(settings)
        .await
        .map_err(to_error)
}

#[tauri::command]
async fn get_stats(state: State<'_, AppState>) -> Result<EngineStats, String> {
    Ok(state.engine.stats())
}

#[tauri::command]
fn bridge_info(state: State<'_, AppState>) -> BridgeInfo {
    state.bridge.clone()
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Err("folder does not exist".into());
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(to_error)?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(to_error)?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(to_error)?;
    }
    Ok(())
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    if !path.exists() {
        return Err("file does not exist".into());
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map_err(to_error)?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(to_error)?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(parent) = path.parent() {
            std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(to_error)?;
        }
    }
    Ok(())
}

#[tauri::command]
async fn crawl_site(
    state: State<'_, AppState>,
    url: String,
    max_depth: Option<u32>,
    max_pages: Option<usize>,
) -> Result<Vec<DiscoveredLink>, String> {
    let settings = state.engine.settings().await;
    let parsed = Url::parse(&url).map_err(to_error)?;
    let client = reqwest::Client::builder()
        .user_agent(settings.user_agent.clone())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(to_error)?;

    let options = CrawlOptions {
        max_depth: max_depth.unwrap_or(2).min(6),
        max_pages: max_pages.unwrap_or(200).min(2000),
        same_host_only: true,
        ..Default::default()
    };

    site::crawl(parsed, options, |page| {
        let client = client.clone();
        async move {
            let response = match client.get(page.clone()).send().await {
                Ok(response) => response,
                Err(_) => return Ok(None),
            };
            if !response.status().is_success() {
                return Ok(None);
            }
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !content_type.contains("text/html") && !content_type.contains("xml") {
                return Ok(None);
            }
            let body = response
                .text()
                .await
                .map_err(|error| hyperfetch_core::Error::failed(error.to_string()))?;
            Ok(Some(body))
        }
    })
    .await
    .map_err(to_error)
}

#[tauri::command]
async fn start_hls(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
    filename: Option<String>,
    headers: Option<HashMap<String, String>>,
) -> Result<String, String> {
    let settings = state.engine.settings().await;
    let task_id = spawn_hls_task(
        &app,
        settings,
        url,
        filename,
        headers.unwrap_or_default(),
        None,
    )?;
    Ok(task_id)
}

#[tauri::command]
async fn list_captures(state: State<'_, AppState>) -> Result<Vec<CaptureOffer>, String> {
    let offers = state.offers.lock().map_err(to_error)?;
    let mut values: Vec<CaptureOffer> = offers.values().cloned().collect();
    values.sort_by_key(|offer| offer.created_at);
    Ok(values)
}

#[tauri::command]
async fn accept_capture(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<CaptureAction, String> {
    let uuid = parse_id(&id)?;
    let offer = state
        .offers
        .lock()
        .map_err(to_error)?
        .get(&uuid)
        .cloned()
        .ok_or_else(|| "capture no longer available".to_string())?;

    let action = accept_offer(&app, &state.engine, offer).await?;
    if let Ok(mut offers) = state.offers.lock() {
        offers.remove(&uuid);
    }
    Ok(action)
}

#[tauri::command]
async fn dismiss_capture(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let uuid = parse_id(&id)?;
    state.offers.lock().map_err(to_error)?.remove(&uuid);
    Ok(())
}

#[tauri::command]
fn list_clipboard_offers(state: State<'_, AppState>) -> Vec<ClipboardOffer> {
    state.clipboard.list()
}

#[tauri::command]
async fn accept_clipboard_offer(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<CaptureAction, String> {
    let uuid = parse_id(&id)?;
    let offer = state
        .clipboard
        .get(uuid)
        .ok_or_else(|| "clipboard offer no longer available".to_string())?;

    let action = if offer.kind == "hls" {
        let settings = state.engine.settings().await;
        let task_id = spawn_hls_task(
            &app,
            settings,
            offer.url.clone(),
            Some(offer.filename.clone()),
            HashMap::new(),
            None,
        )?;
        CaptureAction {
            action: "grabber".into(),
            id: task_id,
            message: Some("HLS download started".into()),
        }
    } else {
        let spec = DownloadSpec {
            url: offer.url.clone(),
            filename: Some(offer.filename.clone()),
            start_now: true,
            ..Default::default()
        };
        let snapshot = state.engine.add(spec).await.map_err(to_error)?;
        CaptureAction {
            action: "download".into(),
            id: snapshot.id.to_string(),
            message: None,
        }
    };

    state.clipboard.remove(uuid);
    Ok(action)
}

#[tauri::command]
fn dismiss_clipboard_offer(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let uuid = parse_id(&id)?;
    state.clipboard.remove(uuid);
    Ok(())
}

async fn accept_offer(
    app: &AppHandle,
    engine: &Engine,
    offer: CaptureOffer,
) -> Result<CaptureAction, String> {
    if offer.kind == "hls" {
        let settings = engine.settings().await;
        let mut headers = HashMap::new();
        if let Some(referer) = &offer.referer {
            headers.insert("Referer".to_string(), referer.clone());
        }
        if let Some(cookie) = &offer.cookie {
            headers.insert("Cookie".to_string(), cookie.clone());
        }
        let task_id = spawn_hls_task(
            app,
            settings,
            offer.url,
            offer.filename,
            headers,
            offer.user_agent,
        )?;
        return Ok(CaptureAction {
            action: "grabber".into(),
            id: task_id,
            message: Some("HLS download started".into()),
        });
    }

    if offer.kind == "dash" {
        return Err(
            "DASH manifests are detected but not downloadable yet; DRM-protected streams are not supported"
                .into(),
        );
    }

    let mut headers = HashMap::new();
    if let Some(referer) = &offer.referer {
        headers.insert("Referer".to_string(), referer.clone());
    }

    let spec = DownloadSpec {
        url: offer.url,
        filename: offer.filename,
        headers,
        cookie: offer.cookie,
        user_agent: offer.user_agent,
        start_now: true,
        ..Default::default()
    };

    let snapshot = engine.add(spec).await.map_err(to_error)?;
    Ok(CaptureAction {
        action: "download".into(),
        id: snapshot.id.to_string(),
        message: None,
    })
}

fn spawn_hls_task(
    app: &AppHandle,
    settings: Settings,
    url: String,
    filename: Option<String>,
    headers: HashMap<String, String>,
    user_agent: Option<String>,
) -> Result<String, String> {
    let parsed = Url::parse(&url).map_err(to_error)?;
    if !parsed.path().to_ascii_lowercase().contains(".m3u8") {
        return Err("not an HLS playlist URL".into());
    }

    let mut name = filename
        .unwrap_or_else(|| filename_from_url(&parsed).unwrap_or_else(|| "video.ts".to_string()));
    name = sanitize_filename(&name);
    let extension = Path::new(&name)
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if extension != "ts" && extension != "m4s" {
        name = Path::new(&name)
            .with_extension("ts")
            .to_string_lossy()
            .into_owned();
    }

    let (dir, _category) = resolve_download_dir(&settings, None, Some("Video"), &name);
    let destination = unique_destination(&dir, &name);

    let task_id = Uuid::new_v4().to_string();
    let app = app.clone();
    let task = task_id.clone();
    let user_agent = user_agent.unwrap_or_else(|| settings.user_agent.clone());
    let connections = settings.connections_per_download.max(1) as usize;

    let mut default_headers = reqwest::header::HeaderMap::new();
    for (name, value) in headers {
        if let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(name.as_bytes()),
            reqwest::header::HeaderValue::from_str(&value),
        ) {
            default_headers.insert(name, value);
        }
    }

    tauri::async_runtime::spawn(async move {
        let client = match reqwest::Client::builder()
            .user_agent(user_agent)
            .default_headers(default_headers)
            .connect_timeout(Duration::from_secs(20))
            .build()
        {
            Ok(client) => client,
            Err(error) => {
                let _ = app.emit(
                    "grabber://error",
                    json!({ "id": task, "message": error.to_string() }),
                );
                return;
            }
        };

        if let Some(parent) = destination.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }

        let progress_app = app.clone();
        let progress_task = task.clone();
        let callback: ProgressCallback = Arc::new(move |done, total, bytes| {
            let _ = progress_app.emit(
                "grabber://progress",
                json!({
                    "id": progress_task,
                    "completed_segments": done,
                    "total_segments": total,
                    "bytes": bytes
                }),
            );
        });

        match download_hls(&client, &parsed, &destination, connections, Some(callback)).await {
            Ok(summary) => {
                let final_path = remux_if_possible(destination.clone()).await;
                let _ = app.emit(
                    "grabber://done",
                    json!({
                        "id": task,
                        "path": final_path.to_string_lossy(),
                        "bytes": summary.bytes,
                        "segments": summary.segments
                    }),
                );
            }
            Err(error) => {
                let _ = tokio::fs::remove_file(&destination).await;
                let _ = app.emit(
                    "grabber://error",
                    json!({ "id": task, "message": error.to_string() }),
                );
            }
        }
    });

    Ok(task_id)
}

async fn remux_if_possible(input: PathBuf) -> PathBuf {
    let extension = input
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if extension != "ts" && extension != "m4s" {
        return input;
    }

    let output = input.with_extension("mp4");
    let source = input.clone();
    let target = output.clone();
    let command_target = target.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        std::process::Command::new("ffmpeg")
            .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
            .arg(&source)
            .args(["-c", "copy"])
            .arg(&command_target)
            .status()
    })
    .await;

    if let Ok(Ok(status)) = result {
        if status.success() && target.exists() {
            let _ = std::fs::remove_file(&input);
            return target;
        }
    }
    input
}

fn authorized(headers: &HeaderMap, token: &str) -> bool {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(|value| value == format!("Bearer {token}"))
        .unwrap_or(false)
}

fn normalize_kind(request: &CaptureRequest) -> String {
    if let Some(kind) = &request.kind {
        let kind = kind.to_ascii_lowercase();
        if matches!(kind.as_str(), "hls" | "dash" | "download" | "video") {
            if kind == "video" {
                return if looks_like_hls(&request.url) {
                    "hls".into()
                } else if looks_like_dash(&request.url) {
                    "dash".into()
                } else {
                    "download".into()
                };
            }
            return kind;
        }
    }
    if looks_like_hls(&request.url) {
        "hls".into()
    } else if looks_like_dash(&request.url) {
        "dash".into()
    } else {
        "download".into()
    }
}

async fn bridge_health() -> Json<Value> {
    Json(json!({
        "app": "HyperFetch",
        "version": env!("CARGO_PKG_VERSION"),
        "requires_token": true
    }))
}

async fn bridge_capture(
    axum::extract::State(ctx): axum::extract::State<BridgeContext>,
    headers: HeaderMap,
    Json(request): Json<CaptureRequest>,
) -> (StatusCode, Json<Value>) {
    if !authorized(&headers, &ctx.token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid or missing bridge token" })),
        );
    }

    let parsed = match Url::parse(&request.url) {
        Ok(url) if matches!(url.scheme(), "http" | "https") => url,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "url must be http(s)" })),
            )
        }
    };

    let kind = normalize_kind(&request);
    let settings = ctx.engine.settings().await;

    let offer = CaptureOffer {
        id: Uuid::new_v4(),
        url: parsed.to_string(),
        filename: request.filename,
        kind,
        referer: request.referer,
        cookie: request.cookie,
        user_agent: request.user_agent,
        page_url: request.page_url,
        created_at: now_ms(),
    };

    if !settings.confirm_captures {
        if offer.kind == "dash" {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "DASH captures are not supported yet" })),
            );
        }
        return match accept_offer(&ctx.app, &ctx.engine, offer).await {
            Ok(action) => {
                let status = if action.action == "grabber" {
                    "started"
                } else {
                    "queued"
                };
                (
                    StatusCode::OK,
                    Json(json!({
                        "status": status,
                        "action": action.action,
                        "id": action.id
                    })),
                )
            }
            Err(error) => (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))),
        };
    }

    if let Ok(mut offers) = ctx.offers.lock() {
        offers.insert(offer.id, offer.clone());
    }
    let _ = ctx.app.emit("capture://offer", &offer);

    (
        StatusCode::OK,
        Json(json!({ "status": "pending", "id": offer.id })),
    )
}

async fn bridge_captures(
    axum::extract::State(ctx): axum::extract::State<BridgeContext>,
    headers: HeaderMap,
) -> (StatusCode, Json<Value>) {
    if !authorized(&headers, &ctx.token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid or missing bridge token" })),
        );
    }
    let offers: Vec<CaptureOffer> = ctx
        .offers
        .lock()
        .map(|offers| offers.values().cloned().collect())
        .unwrap_or_default();
    (StatusCode::OK, Json(json!({ "offers": offers })))
}

async fn bind_bridge(preferred: u16) -> std::io::Result<(TcpListener, u16)> {
    for offset in 0..20u16 {
        let port = preferred.saturating_add(offset);
        if let Ok(listener) = TcpListener::bind(("127.0.0.1", port)).await {
            return Ok((listener, port));
        }
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

fn load_or_create_token(config_dir: &Path) -> std::io::Result<String> {
    std::fs::create_dir_all(config_dir)?;
    let path = config_dir.join("bridge.json");
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(value) = serde_json::from_str::<Value>(&content) {
            if let Some(token) = value.get("token").and_then(Value::as_str) {
                if token.len() >= 16 {
                    return Ok(token.to_string());
                }
            }
        }
    }
    let token = Uuid::new_v4().to_string();
    let payload = serde_json::to_string_pretty(&json!({ "token": token })).unwrap();
    std::fs::write(&path, payload)?;
    Ok(token)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hyperfetch=info".into()),
        )
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let config_dir = app.path().app_config_dir()?;
            let token = load_or_create_token(&config_dir)?;

            let db_path = data_dir.join("hyperfetch.db");
            let engine = tauri::async_runtime::block_on(Engine::open(&db_path, None))?;
            let settings = tauri::async_runtime::block_on(engine.settings());

            let (listener, port) =
                tauri::async_runtime::block_on(bind_bridge(settings.bridge_port))?;

            let bridge_url = format!("http://127.0.0.1:{port}");
            let offers: Arc<Mutex<HashMap<Uuid, CaptureOffer>>> =
                Arc::new(Mutex::new(HashMap::new()));
            let clipboard_state = Arc::new(ClipboardState::default());

            let context = BridgeContext {
                engine: engine.clone(),
                token: Arc::new(token.clone()),
                offers: offers.clone(),
                app: app_handle.clone(),
            };

            let router = Router::new()
                .route("/v1/health", get(bridge_health))
                .route("/v1/capture", post(bridge_capture))
                .route("/v1/captures", get(bridge_captures))
                .layer(
                    CorsLayer::new()
                        .allow_origin(Any)
                        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                        .allow_headers(Any),
                )
                .with_state(context);

            tauri::async_runtime::spawn(async move {
                if let Err(error) = axum::serve(listener, router).await {
                    tracing::error!("bridge server stopped: {error}");
                }
            });

            app.manage(AppState {
                engine: engine.clone(),
                bridge: BridgeInfo {
                    url: bridge_url,
                    port,
                    token,
                },
                offers,
                clipboard: clipboard_state.clone(),
            });

            clipboard::spawn(app_handle.clone(), engine.clone(), clipboard_state);

            let event_engine = engine.clone();
            let event_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let mut receiver = event_engine.subscribe();
                loop {
                    match receiver.recv().await {
                        Ok(event) => {
                            let _ = event_handle.emit("engine://event", &event);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::debug!("dropped {skipped} engine events");
                        }
                        Err(_) => break,
                    }
                }
            });

            tracing::info!("HyperFetch bridge listening on http://127.0.0.1:{port}");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            probe_url,
            add_download,
            list_downloads,
            get_download,
            pause_download,
            resume_download,
            cancel_download,
            remove_download,
            pause_all,
            resume_all,
            clear_completed,
            get_settings,
            update_settings,
            get_stats,
            bridge_info,
            open_folder,
            open_path,
            crawl_site,
            start_hls,
            list_captures,
            accept_capture,
            dismiss_capture,
            list_clipboard_offers,
            accept_clipboard_offer,
            dismiss_clipboard_offer
        ])
        .build(tauri::generate_context!())
        .expect("error while building HyperFetch")
        .run(|app_handle, event| {
            if let RunEvent::ExitRequested { .. } = event {
                if let Some(state) = app_handle.try_state::<AppState>() {
                    tauri::async_runtime::block_on(state.engine.shutdown());
                }
            }
        });
}
