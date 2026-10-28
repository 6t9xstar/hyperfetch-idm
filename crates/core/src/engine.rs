use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use reqwest::{Client, Response, StatusCode};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{broadcast, Notify, RwLock};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use url::Url;
use uuid::Uuid;

use crate::db::Db;
use crate::error::{Error, Result};
use crate::plan::{BatchJob, RangePlan};
use crate::probe::{apply_headers, probe_with_client, ProbeResult};
use crate::ratelimit::RateLimiter;
use crate::types::*;
use crate::util::{
    now_ms, resolve_download_dir, sanitize_filename, unique_destination, validators_differ,
};

const MONITOR_INTERVAL: Duration = Duration::from_millis(500);
const WRITE_BUFFER: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EngineEvent {
    Added { download: DownloadSnapshot },
    Updated { download: DownloadSnapshot },
    Progress { progress: ProgressEvent },
    Removed { id: Uuid },
    Queue { pending: usize, active: usize },
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineStats {
    pub active: usize,
    pub queued: usize,
    pub speed: u64,
}

/// The download engine. Cheap to clone; all clones share the same state.
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

struct Inner {
    db: Arc<Db>,
    settings: RwLock<Settings>,
    clients: Mutex<HashMap<Option<String>, Client>>,
    events: broadcast::Sender<EngineEvent>,
    runtimes: Mutex<HashMap<Uuid, Arc<Runtime>>>,
    queue: Mutex<VecDeque<Uuid>>,
    running: AtomicUsize,
    wake: Arc<Notify>,
    global_limiter: Arc<RateLimiter>,
    shutdown: CancellationToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopReason {
    Pause,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionOutcome {
    Completed,
    Paused,
    Canceled,
}

struct Runtime {
    progress: Arc<Progress>,
    token: Mutex<CancellationToken>,
    reason: Mutex<Option<StopReason>>,
    active: AtomicBool,
    generation: AtomicU64,
}

struct Progress {
    received: AtomicU64,
    total: AtomicU64,
    speed: AtomicU64,
    active_connections: AtomicU32,
}

impl Runtime {
    fn new() -> Self {
        Self {
            progress: Arc::new(Progress {
                received: AtomicU64::new(0),
                total: AtomicU64::new(0),
                speed: AtomicU64::new(0),
                active_connections: AtomicU32::new(0),
            }),
            token: Mutex::new(CancellationToken::new()),
            reason: Mutex::new(None),
            active: AtomicBool::new(false),
            generation: AtomicU64::new(0),
        }
    }

    fn begin(&self, received: u64, total: u64) -> (CancellationToken, u64) {
        let token = CancellationToken::new();
        *self.token.lock() = token.clone();
        *self.reason.lock() = None;
        self.progress.received.store(received, Ordering::SeqCst);
        self.progress.total.store(total, Ordering::SeqCst);
        self.progress.speed.store(0, Ordering::SeqCst);
        self.progress.active_connections.store(0, Ordering::SeqCst);
        self.active.store(true, Ordering::SeqCst);
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        (token, generation)
    }

    fn stop(&self, reason: StopReason) {
        *self.reason.lock() = Some(reason);
        self.token.lock().cancel();
    }

    fn outcome(&self) -> SessionOutcome {
        match *self.reason.lock() {
            Some(StopReason::Cancel) => SessionOutcome::Canceled,
            _ => SessionOutcome::Paused,
        }
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    fn finish(&self, generation: u64) {
        if self.generation.load(Ordering::SeqCst) == generation {
            self.active.store(false, Ordering::SeqCst);
            self.progress.speed.store(0, Ordering::SeqCst);
            self.progress.active_connections.store(0, Ordering::SeqCst);
        }
    }
}

impl Engine {
    /// Open (or create) the persistent database and recover unfinished downloads.
    pub async fn open(db_path: impl AsRef<Path>, settings: Option<Settings>) -> Result<Self> {
        let db = Arc::new(Db::open(db_path.as_ref())?);
        let settings = match settings {
            Some(settings) => settings,
            None => db.load_settings()?.unwrap_or_default(),
        };
        let _ = std::fs::create_dir_all(&settings.download_dir);

        let (events, _) = broadcast::channel(2048);
        let global_limiter = Arc::new(RateLimiter::new(settings.global_speed_limit));
        let auto_resume = settings.auto_resume;

        let engine = Self {
            inner: Arc::new(Inner {
                db,
                settings: RwLock::new(settings),
                clients: Mutex::new(HashMap::new()),
                events,
                runtimes: Mutex::new(HashMap::new()),
                queue: Mutex::new(VecDeque::new()),
                running: AtomicUsize::new(0),
                wake: Arc::new(Notify::new()),
                global_limiter,
                shutdown: CancellationToken::new(),
            }),
        };

        engine.recover(auto_resume)?;

        let weak = Arc::downgrade(&engine.inner);
        tokio::spawn(async move { scheduler_task(weak).await });
        engine.wake();

        Ok(engine)
    }

    fn recover(&self, auto_resume: bool) -> Result<()> {
        let unfinished = self.inner.db.list_unfinished()?;
        for record in unfinished {
            let runtime = Arc::new(Runtime::new());
            runtime
                .progress
                .total
                .store(record.total_size, Ordering::SeqCst);
            runtime
                .progress
                .received
                .store(record.received, Ordering::SeqCst);
            self.inner.runtimes.lock().insert(record.id, runtime);

            let state = if auto_resume {
                DownloadState::Queued
            } else {
                DownloadState::Paused
            };
            self.inner.db.set_state(record.id, state, None)?;
            if auto_resume {
                self.inner.queue.lock().push_back(record.id);
            }
        }
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.inner.events.subscribe()
    }

    pub async fn settings(&self) -> Settings {
        self.inner.settings.read().await.clone()
    }

    pub async fn update_settings(&self, settings: Settings) -> Result<Settings> {
        let _ = std::fs::create_dir_all(&settings.download_dir);
        self.inner.db.save_settings(&settings)?;
        self.inner
            .global_limiter
            .set_rate(settings.global_speed_limit);
        *self.inner.settings.write().await = settings.clone();
        self.wake();
        Ok(settings)
    }

    pub async fn probe(
        &self,
        url: &str,
        headers: Option<HashMap<String, String>>,
        proxy: Option<String>,
    ) -> Result<ProbeResult> {
        let parsed = Url::parse(url.trim())?;
        let settings = self.settings().await;
        let client = self
            .client_for(
                proxy
                    .filter(|proxy| !proxy.trim().is_empty())
                    .or(settings.proxy.clone()),
            )
            .await?;
        let mut headers = headers.unwrap_or_default();
        headers
            .entry("User-Agent".to_string())
            .or_insert(settings.user_agent);
        probe_with_client(&client, &parsed, &headers).await
    }

    /// Probe a URL and persist a new download.
    pub async fn add(&self, spec: DownloadSpec) -> Result<DownloadSnapshot> {
        if spec.url.trim().is_empty() {
            return Err(Error::InvalidInput("URL is required".into()));
        }

        let url = Url::parse(spec.url.trim())?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(Error::InvalidInput(format!(
                "unsupported URL scheme '{}'",
                url.scheme()
            )));
        }

        let settings = self.settings().await;
        let proxy = spec
            .proxy
            .clone()
            .filter(|proxy| !proxy.trim().is_empty())
            .or_else(|| settings.proxy.clone());
        let client = self.client_for(proxy.clone()).await?;

        let mut headers = spec.headers.clone();
        headers.remove("User-Agent");
        headers.remove("Cookie");
        let user_agent = spec
            .user_agent
            .clone()
            .unwrap_or_else(|| settings.user_agent.clone());

        let mut request_headers = headers.clone();
        request_headers.insert("User-Agent".to_string(), user_agent.clone());
        if let Some(cookie) = &spec.cookie {
            request_headers.insert("Cookie".to_string(), cookie.clone());
        }

        let probe = probe_with_client(&client, &url, &request_headers).await?;

        let requested_name = spec
            .filename
            .as_deref()
            .map(sanitize_filename)
            .unwrap_or_else(|| sanitize_filename(&probe.filename));
        let (dir, category) = resolve_download_dir(
            &settings,
            spec.dir.as_deref(),
            spec.category.as_deref(),
            &requested_name,
        );
        tokio::fs::create_dir_all(&dir).await?;
        let destination = unique_destination(&dir, &requested_name);
        let filename = destination
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or(requested_name);

        let connections = spec
            .connections
            .unwrap_or(settings.connections_per_download)
            .clamp(1, settings.max_connections.max(1));

        let now = now_ms();
        let record = DownloadRecord {
            id: Uuid::new_v4(),
            url: url.to_string(),
            final_url: Some(probe.final_url),
            filename,
            dir: dir.to_string_lossy().into_owned(),
            total_size: probe.total_size,
            received: 0,
            state: if spec.start_now {
                DownloadState::Queued
            } else {
                DownloadState::Paused
            },
            etag: probe.etag,
            last_modified: probe.last_modified,
            content_type: probe.content_type,
            accept_ranges: probe.accept_ranges,
            connections,
            speed_limit: spec.speed_limit,
            category,
            priority: spec.priority,
            error: None,
            sha256: None,
            headers,
            user_agent: Some(user_agent),
            cookie: spec.cookie,
            proxy,
            created_at: now,
            updated_at: now,
            finished_at: None,
        };

        self.inner.db.insert_download(&record)?;

        let runtime = Arc::new(Runtime::new());
        runtime
            .progress
            .total
            .store(record.total_size, Ordering::SeqCst);
        self.inner.runtimes.lock().insert(record.id, runtime);

        let snapshot = build_snapshot(&self.inner, &record);
        self.emit(EngineEvent::Added {
            download: snapshot.clone(),
        });

        if spec.start_now {
            self.enqueue(record.id);
        }

        Ok(snapshot)
    }

    pub async fn get(&self, id: Uuid) -> Result<DownloadSnapshot> {
        let record = self.inner.db.get_download(id)?.ok_or(Error::NotFound)?;
        Ok(build_snapshot(&self.inner, &record))
    }

    pub async fn list(&self) -> Result<Vec<DownloadSnapshot>> {
        let records = self.inner.db.list_downloads()?;
        Ok(records
            .iter()
            .map(|record| build_snapshot(&self.inner, record))
            .collect())
    }

    pub fn url_exists(&self, url: &str) -> Result<bool> {
        self.inner.db.url_exists(url)
    }

    pub fn stats(&self) -> EngineStats {
        let speed = self
            .inner
            .runtimes
            .lock()
            .values()
            .filter(|runtime| runtime.is_active())
            .map(|runtime| runtime.progress.speed.load(Ordering::Relaxed))
            .sum();

        EngineStats {
            active: self.inner.running.load(Ordering::SeqCst),
            queued: self.inner.queue.lock().len(),
            speed,
        }
    }

    pub async fn pause(&self, id: Uuid) -> Result<()> {
        let record = self.inner.db.get_download(id)?.ok_or(Error::NotFound)?;
        if record.state.is_terminal() {
            return Err(Error::InvalidState(format!(
                "cannot pause a download in state '{}'",
                record.state
            )));
        }

        self.remove_from_queue(id);

        if let Some(runtime) = self.runtime_for(id) {
            runtime.stop(StopReason::Pause);
            for _ in 0..100 {
                if !runtime.is_active() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        self.inner.db.set_state(id, DownloadState::Paused, None)?;
        self.emit_updated(id);
        self.emit_queue();
        Ok(())
    }

    pub async fn resume(&self, id: Uuid) -> Result<()> {
        let record = self.inner.db.get_download(id)?.ok_or(Error::NotFound)?;
        if record.state == DownloadState::Done {
            return Err(Error::InvalidState("download already completed".into()));
        }
        if let Some(runtime) = self.runtime_for(id) {
            if runtime.is_active() {
                return Ok(());
            }
        }

        self.inner.db.set_state(id, DownloadState::Queued, None)?;
        self.emit_updated(id);
        self.enqueue(id);
        Ok(())
    }

    pub async fn cancel(&self, id: Uuid) -> Result<()> {
        let record = self.inner.db.get_download(id)?.ok_or(Error::NotFound)?;
        if record.state == DownloadState::Done {
            return Err(Error::InvalidState(
                "completed downloads cannot be cancelled".into(),
            ));
        }

        self.remove_from_queue(id);

        if let Some(runtime) = self.runtime_for(id) {
            runtime.stop(StopReason::Cancel);
            for _ in 0..100 {
                if !runtime.is_active() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        self.cleanup_canceled(id).await;
        self.emit_queue();
        Ok(())
    }

    pub async fn remove(&self, id: Uuid, delete_file: bool) -> Result<()> {
        let record = self.inner.db.get_download(id)?.ok_or(Error::NotFound)?;
        self.remove_from_queue(id);

        if let Some(runtime) = self.runtime_for(id) {
            runtime.stop(StopReason::Cancel);
            for _ in 0..100 {
                if !runtime.is_active() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        let part = record.part_path();
        let _ = tokio::fs::remove_file(&part).await;

        if delete_file {
            let final_path = record.final_path();
            let _ = tokio::fs::remove_file(&final_path).await;
        }

        self.inner.db.delete_download(id)?;
        self.inner.runtimes.lock().remove(&id);
        self.emit(EngineEvent::Removed { id });
        self.emit_queue();
        Ok(())
    }

    pub async fn pause_all(&self) -> Result<()> {
        let ids: Vec<Uuid> = self
            .inner
            .db
            .list_downloads()?
            .into_iter()
            .filter(|record| record.state.is_unfinished())
            .map(|record| record.id)
            .collect();
        for id in ids {
            let _ = self.pause(id).await;
        }
        Ok(())
    }

    pub async fn resume_all(&self) -> Result<()> {
        let ids: Vec<Uuid> = self
            .inner
            .db
            .list_downloads()?
            .into_iter()
            .filter(|record| matches!(record.state, DownloadState::Paused | DownloadState::Error))
            .map(|record| record.id)
            .collect();
        for id in ids {
            let _ = self.resume(id).await;
        }
        Ok(())
    }

    pub async fn clear_completed(&self) -> Result<()> {
        let ids: Vec<Uuid> = self
            .inner
            .db
            .list_downloads()?
            .into_iter()
            .filter(|record| record.state == DownloadState::Done)
            .map(|record| record.id)
            .collect();
        for id in ids {
            let _ = self.remove(id, false).await;
        }
        Ok(())
    }

    /// Stop all workers while keeping resumable state. Used on application exit.
    pub async fn shutdown(&self) {
        self.inner.shutdown.cancel();
        self.inner.queue.lock().clear();

        let runtimes: Vec<Arc<Runtime>> = self.inner.runtimes.lock().values().cloned().collect();
        for runtime in runtimes {
            runtime.stop(StopReason::Pause);
        }

        for _ in 0..200 {
            if self.inner.running.load(Ordering::SeqCst) == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    pub fn wake(&self) {
        self.inner.wake.notify_one();
    }

    /// Compatibility helper: schedules a scheduling pass.
    pub async fn pump(&self) {
        self.wake();
    }

    async fn pump_once(&self) {
        let settings = self.inner.settings.read().await.clone();

        loop {
            if self.inner.shutdown.is_cancelled() {
                break;
            }
            if self.inner.running.load(Ordering::SeqCst) >= settings.max_concurrent.max(1) {
                break;
            }

            let Some(id) = self.inner.queue.lock().pop_front() else {
                break;
            };

            let record = match self.inner.db.get_download(id) {
                Ok(Some(record)) => record,
                _ => continue,
            };

            if record.state != DownloadState::Queued {
                continue;
            }

            self.inner.running.fetch_add(1, Ordering::SeqCst);
            let engine = self.clone();
            tokio::spawn(async move {
                let result = engine.run_download(id).await;
                engine.inner.running.fetch_sub(1, Ordering::SeqCst);
                if let Err(error) = result {
                    tracing::warn!(download = %id, "download failed: {error}");
                }
                engine.emit_queue();
                engine.wake();
            });
        }

        self.emit_queue();
    }

    async fn run_download(&self, id: Uuid) -> Result<()> {
        let record = self.inner.db.get_download(id)?.ok_or(Error::NotFound)?;

        if matches!(record.state, DownloadState::Done | DownloadState::Canceled) {
            return Ok(());
        }

        let runtime = self.ensure_runtime(&record);
        let (token, generation) = runtime.begin(record.received, record.total_size);

        self.set_state(id, DownloadState::Probing, None)?;

        let monitor_cancel = CancellationToken::new();
        let monitor = tokio::spawn(monitor_progress(
            self.clone(),
            id,
            runtime.clone(),
            monitor_cancel.clone(),
        ));

        let result = self.download_session(&record, &runtime, &token).await;

        monitor_cancel.cancel();
        let _ = monitor.await;

        let final_result = match result {
            Ok(SessionOutcome::Completed) => {
                self.emit_updated(id);
                Ok(())
            }
            Ok(SessionOutcome::Paused) => {
                self.inner.db.set_state(id, DownloadState::Paused, None)?;
                self.emit_updated(id);
                Ok(())
            }
            Ok(SessionOutcome::Canceled) => {
                self.cleanup_canceled(id).await;
                Ok(())
            }
            Err(error) => {
                if token.is_cancelled() {
                    match runtime.outcome() {
                        SessionOutcome::Canceled => {
                            self.cleanup_canceled(id).await;
                            Ok(())
                        }
                        _ => {
                            self.inner.db.set_state(id, DownloadState::Paused, None)?;
                            self.emit_updated(id);
                            Ok(())
                        }
                    }
                } else {
                    let message = error.to_string();
                    self.inner
                        .db
                        .set_state(id, DownloadState::Error, Some(&message))?;
                    self.emit_updated(id);
                    Err(error)
                }
            }
        };

        runtime.finish(generation);
        final_result
    }

    async fn download_session(
        &self,
        record: &DownloadRecord,
        runtime: &Arc<Runtime>,
        token: &CancellationToken,
    ) -> Result<SessionOutcome> {
        let settings = self.settings().await;
        let client = self
            .client_for(record.proxy.clone().or_else(|| settings.proxy.clone()))
            .await?;

        let url = Url::parse(&record.url)?;
        let mut headers = record.request_headers();
        headers
            .entry("User-Agent".to_string())
            .or_insert_with(|| settings.user_agent.clone());

        let probe = probe_with_client(&client, &url, &headers).await?;
        if token.is_cancelled() {
            return Ok(runtime.outcome());
        }

        let total = probe.total_size;
        let accept_ranges = probe.accept_ranges;

        let changed = validators_differ(
            record.etag.as_deref(),
            record.last_modified.as_deref(),
            probe.etag.as_deref(),
            probe.last_modified.as_deref(),
        ) || (record.total_size != 0 && total != 0 && record.total_size != total);

        if changed {
            self.inner.db.clear_ranges(record.id)?;
            self.inner.db.update_received(record.id, 0)?;
            runtime.progress.received.store(0, Ordering::SeqCst);
        }

        self.inner.db.update_meta(
            record.id,
            Some(&probe.final_url),
            total,
            accept_ranges,
            probe.etag.as_deref(),
            probe.last_modified.as_deref(),
            probe.content_type.as_deref(),
        )?;
        runtime.progress.total.store(total, Ordering::SeqCst);

        let mut record = record.clone();
        record.total_size = total;
        record.accept_ranges = accept_ranges;
        record.etag = probe.etag.clone().or(record.etag);
        record.last_modified = probe.last_modified.clone().or(record.last_modified);

        self.set_state(record.id, DownloadState::Running, None)?;

        let part_path = record.part_path();
        prepare_part(&part_path, total, false).await?;

        let use_segments = accept_ranges && total >= settings.min_segmented_size && total > 0;

        if use_segments {
            match self
                .run_segmented(
                    &record, runtime, token, &client, &url, &headers, &part_path, &settings,
                )
                .await
            {
                Ok(()) => {}
                Err(Error::RangeUnsupported) => {
                    tracing::info!(download = %record.id, "server ignored range requests; falling back to single stream");
                    reset_for_single(&self.inner.db, &record, &part_path, total).await?;
                    runtime.progress.received.store(0, Ordering::SeqCst);
                    self.run_single(
                        &record, runtime, token, &client, &url, &headers, total, false, &part_path,
                        &settings,
                    )
                    .await?;
                }
                Err(Error::Cancelled) => return Ok(runtime.outcome()),
                Err(error) => return Err(error),
            }
        } else {
            self.run_single(
                &record,
                runtime,
                token,
                &client,
                &url,
                &headers,
                total,
                accept_ranges,
                &part_path,
                &settings,
            )
            .await?;
        }

        finalize_download(&self.inner, &record, runtime, &part_path, total, &settings).await?;
        Ok(SessionOutcome::Completed)
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_segmented(
        &self,
        record: &DownloadRecord,
        runtime: &Arc<Runtime>,
        token: &CancellationToken,
        client: &Client,
        url: &Url,
        headers: &HashMap<String, String>,
        part_path: &Path,
        settings: &Settings,
    ) -> Result<()> {
        let total = record.total_size;
        let rows = self.inner.db.load_ranges(record.id)?;
        let mut plan = if plan_rows_valid(&rows, total) {
            RangePlan::from_rows(
                rows,
                record.connections,
                settings.batch_size,
                settings.min_split_size,
            )
        } else {
            self.inner.db.clear_ranges(record.id)?;
            RangePlan::new(
                total,
                record.connections,
                settings.batch_size,
                settings.min_split_size,
            )
        };

        if plan.total_size() != total {
            self.inner.db.clear_ranges(record.id)?;
            plan = RangePlan::new(
                total,
                record.connections,
                settings.batch_size,
                settings.min_split_size,
            );
        }

        let initial_received = plan.received();
        runtime
            .progress
            .received
            .store(initial_received, Ordering::SeqCst);
        self.inner.db.update_received(record.id, initial_received)?;

        if plan.is_complete() {
            return Ok(());
        }

        let plan = Arc::new(Mutex::new(plan));
        let notify = Arc::new(Notify::new());
        let fatal = Arc::new(AtomicBool::new(false));
        let range_unsupported = Arc::new(AtomicBool::new(false));
        let error_slot: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let persist = Arc::new(PersistThrottle::new(Duration::from_millis(400)));

        let ctx = SegmentedContext {
            db: self.inner.db.clone(),
            client: client.clone(),
            url: url.clone(),
            headers: headers.clone(),
            if_range: record.etag.clone().or_else(|| record.last_modified.clone()),
            id: record.id,
            part: part_path.to_path_buf(),
            retries: settings.batch_retries.max(1),
            token: token.clone(),
            plan: plan.clone(),
            notify: notify.clone(),
            fatal: fatal.clone(),
            range_unsupported: range_unsupported.clone(),
            error_slot: error_slot.clone(),
            progress: runtime.progress.clone(),
            limiter: Arc::new(RateLimiter::new(record.speed_limit)),
            global_limiter: self.inner.global_limiter.clone(),
            persist: persist.clone(),
        };

        let worker_count = record.connections.max(1) as usize;
        let mut workers = JoinSet::new();
        for _ in 0..worker_count {
            let ctx = ctx.clone();
            workers.spawn(async move { segmented_worker(ctx).await });
        }

        while workers.join_next().await.is_some() {}

        persist_plan(&ctx, true);

        if range_unsupported.load(Ordering::SeqCst) {
            return Err(Error::RangeUnsupported);
        }
        if fatal.load(Ordering::SeqCst) {
            let message = error_slot
                .lock()
                .clone()
                .unwrap_or_else(|| "download failed".to_string());
            return Err(Error::failed(message));
        }
        if token.is_cancelled() {
            return Err(Error::Cancelled);
        }

        let complete = plan.lock().is_complete();
        if !complete {
            let received = plan.lock().received();
            return Err(Error::failed(format!(
                "download stopped early ({received}/{total} bytes)"
            )));
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_single(
        &self,
        record: &DownloadRecord,
        runtime: &Arc<Runtime>,
        token: &CancellationToken,
        client: &Client,
        url: &Url,
        headers: &HashMap<String, String>,
        total: u64,
        accept_ranges: bool,
        part_path: &Path,
        settings: &Settings,
    ) -> Result<()> {
        let metadata = tokio::fs::metadata(part_path).await.ok();
        let existing_len = metadata.map(|metadata| metadata.len()).unwrap_or(0);
        let mut offset = if accept_ranges {
            record.received.min(existing_len)
        } else {
            0
        };

        if offset == 0 {
            let file = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(part_path)
                .await?;
            if total > 0 {
                file.set_len(total).await?;
            }
        }

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(part_path)
            .await?;

        if total > 0 && existing_len != total {
            file.set_len(total).await?;
        }

        runtime.progress.received.store(offset, Ordering::SeqCst);
        runtime.progress.total.store(total, Ordering::SeqCst);
        self.inner.db.update_received(record.id, offset)?;

        let limiter = RateLimiter::new(record.speed_limit);
        let mut attempts = 0u32;
        let mut last_persist = Instant::now();
        let max_attempts = settings.batch_retries.max(1) * 4;

        loop {
            if token.is_cancelled() {
                self.inner.db.update_received(record.id, offset)?;
                return Err(Error::Cancelled);
            }

            let mut request = apply_headers(client.get(url.clone()), headers);
            if offset > 0 && accept_ranges {
                request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
                if let Some(validator) =
                    record.etag.clone().or_else(|| record.last_modified.clone())
                {
                    request = request.header("If-Range", validator);
                }
            }

            let response = tokio::select! {
                _ = token.cancelled() => {
                    self.inner.db.update_received(record.id, offset)?;
                    return Err(Error::Cancelled);
                }
                result = request.send() => result,
            };

            let response = match response {
                Ok(response) => response,
                Err(error) => {
                    attempts += 1;
                    if attempts > max_attempts {
                        return Err(Error::failed(format!("network error: {error}")));
                    }
                    sleep_backoff(attempts, token).await?;
                    continue;
                }
            };

            let status = response.status();

            if status == StatusCode::RANGE_NOT_SATISFIABLE {
                if total > 0 && offset >= total {
                    return Ok(());
                }
                return Err(Error::failed("server rejected the requested range"));
            }

            if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
                attempts += 1;
                if attempts > max_attempts {
                    return Err(Error::failed(format!(
                        "server responded with HTTP {status}"
                    )));
                }
                sleep_backoff(attempts, token).await?;
                continue;
            }

            if !status.is_success() {
                return Err(Error::failed(format!(
                    "server responded with HTTP {status}"
                )));
            }

            if status == StatusCode::OK && offset > 0 {
                offset = 0;
                runtime.progress.received.store(0, Ordering::SeqCst);
                self.inner.db.update_received(record.id, 0)?;
                file.set_len(0).await?;
                continue;
            }

            if status == StatusCode::PARTIAL_CONTENT {
                if let Some(start) = content_range_start(&response) {
                    if start != offset {
                        return Err(Error::failed(format!(
                            "server returned range starting at {start}, expected {offset}"
                        )));
                    }
                }
            }

            let mut response = response;
            let mut written = 0u64;
            let mut failed: Option<String> = None;

            loop {
                let chunk = tokio::select! {
                    _ = token.cancelled() => {
                        offset += written;
                        runtime.progress.received.store(offset, Ordering::SeqCst);
                        self.inner.db.update_received(record.id, offset)?;
                        return Err(Error::Cancelled);
                    }
                    chunk = response.chunk() => chunk,
                };

                match chunk {
                    Ok(Some(bytes)) => {
                        if bytes.is_empty() {
                            continue;
                        }
                        if total > 0 && offset + written + bytes.len() as u64 > total {
                            return Err(Error::failed("server sent more data than expected"));
                        }

                        file.seek(std::io::SeekFrom::Start(offset + written))
                            .await?;
                        file.write_all(&bytes).await?;
                        written += bytes.len() as u64;
                        runtime
                            .progress
                            .received
                            .store(offset + written, Ordering::SeqCst);

                        self.inner.global_limiter.acquire(bytes.len() as u64).await;
                        limiter.acquire(bytes.len() as u64).await;

                        if last_persist.elapsed() >= Duration::from_secs(1) {
                            self.inner.db.update_received(record.id, offset + written)?;
                            last_persist = Instant::now();
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        failed = Some(error.to_string());
                        break;
                    }
                }
            }

            offset += written;
            runtime.progress.received.store(offset, Ordering::SeqCst);
            self.inner.db.update_received(record.id, offset)?;
            last_persist = Instant::now();

            if let Some(error) = failed {
                attempts += 1;
                if attempts > max_attempts {
                    return Err(Error::failed(format!("network error: {error}")));
                }
                if !accept_ranges {
                    offset = 0;
                    runtime.progress.received.store(0, Ordering::SeqCst);
                    self.inner.db.update_received(record.id, 0)?;
                    file.set_len(0).await?;
                }
                sleep_backoff(attempts, token).await?;
                continue;
            }

            if total > 0 {
                if offset >= total {
                    return Ok(());
                }
                attempts += 1;
                if attempts > max_attempts {
                    return Err(Error::failed(format!(
                        "connection closed early ({offset}/{total} bytes)"
                    )));
                }
                if !accept_ranges {
                    offset = 0;
                    runtime.progress.received.store(0, Ordering::SeqCst);
                    self.inner.db.update_received(record.id, 0)?;
                    file.set_len(0).await?;
                }
                sleep_backoff(attempts, token).await?;
                continue;
            }

            if offset > 0 {
                runtime.progress.total.store(offset, Ordering::SeqCst);
                self.inner.db.update_meta(
                    record.id,
                    None,
                    offset,
                    accept_ranges,
                    None,
                    None,
                    None,
                )?;
            }
            return Ok(());
        }
    }

    async fn client_for(&self, proxy: Option<String>) -> Result<Client> {
        let key = proxy.filter(|proxy| !proxy.trim().is_empty());
        if let Some(client) = self.inner.clients.lock().get(&key).cloned() {
            return Ok(client);
        }

        let mut builder = Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::limited(10))
            .pool_max_idle_per_host(32)
            .tcp_nodelay(true);

        if let Some(proxy_url) = &key {
            let proxy = reqwest::Proxy::all(proxy_url).map_err(|error| {
                Error::InvalidInput(format!("invalid proxy '{proxy_url}': {error}"))
            })?;
            builder = builder.proxy(proxy);
        }

        let client = builder.build()?;
        self.inner.clients.lock().insert(key, client.clone());
        Ok(client)
    }

    fn runtime_for(&self, id: Uuid) -> Option<Arc<Runtime>> {
        self.inner.runtimes.lock().get(&id).cloned()
    }

    fn ensure_runtime(&self, record: &DownloadRecord) -> Arc<Runtime> {
        let mut runtimes = self.inner.runtimes.lock();
        if let Some(runtime) = runtimes.get(&record.id) {
            return runtime.clone();
        }
        let runtime = Arc::new(Runtime::new());
        runtime
            .progress
            .total
            .store(record.total_size, Ordering::SeqCst);
        runtime
            .progress
            .received
            .store(record.received, Ordering::SeqCst);
        runtimes.insert(record.id, runtime.clone());
        runtime
    }

    fn enqueue(&self, id: Uuid) {
        {
            let mut queue = self.inner.queue.lock();
            if !queue.contains(&id) {
                queue.push_back(id);
            }
        }
        self.wake();
    }

    fn remove_from_queue(&self, id: Uuid) {
        self.inner.queue.lock().retain(|queued| *queued != id);
    }

    fn set_state(&self, id: Uuid, state: DownloadState, error: Option<&str>) -> Result<()> {
        self.inner.db.set_state(id, state, error)?;
        self.emit_updated(id);
        Ok(())
    }

    fn emit_updated(&self, id: Uuid) {
        if let Ok(Some(record)) = self.inner.db.get_download(id) {
            self.emit(EngineEvent::Updated {
                download: build_snapshot(&self.inner, &record),
            });
        }
    }

    fn emit_queue(&self) {
        let _ = self.inner.events.send(EngineEvent::Queue {
            pending: self.inner.queue.lock().len(),
            active: self.inner.running.load(Ordering::SeqCst),
        });
    }

    fn emit(&self, event: EngineEvent) {
        let _ = self.inner.events.send(event);
    }

    async fn cleanup_canceled(&self, id: Uuid) {
        if let Ok(Some(record)) = self.inner.db.get_download(id) {
            let _ = tokio::fs::remove_file(record.part_path()).await;
        }
        let _ = self.inner.db.clear_ranges(id);
        let _ = self.inner.db.set_state(id, DownloadState::Canceled, None);
        if let Some(runtime) = self.runtime_for(id) {
            runtime.progress.received.store(0, Ordering::SeqCst);
        }
        self.emit_updated(id);
    }
}

fn build_snapshot(inner: &Inner, record: &DownloadRecord) -> DownloadSnapshot {
    let runtime = inner.runtimes.lock().get(&record.id).cloned();
    let live_received = runtime
        .as_ref()
        .map(|runtime| runtime.progress.received.load(Ordering::SeqCst))
        .unwrap_or(0);
    let speed = runtime
        .as_ref()
        .map(|runtime| runtime.progress.speed.load(Ordering::SeqCst))
        .unwrap_or(0);
    let active_connections = runtime
        .as_ref()
        .map(|runtime| runtime.progress.active_connections.load(Ordering::SeqCst))
        .unwrap_or(0);

    let received = if matches!(
        record.state,
        DownloadState::Running | DownloadState::Probing
    ) {
        record.received.max(live_received)
    } else {
        record.received
    };

    let speed = if matches!(record.state, DownloadState::Running) {
        speed
    } else {
        0
    };
    let active_connections = if record.state == DownloadState::Running {
        active_connections
    } else {
        0
    };

    let eta_secs = if speed > 0 && record.total_size > received {
        Some((record.total_size - received) / speed)
    } else {
        None
    };

    DownloadSnapshot {
        id: record.id,
        url: record.url.clone(),
        final_url: record.final_url.clone(),
        filename: record.filename.clone(),
        dir: record.dir.clone(),
        total_size: record.total_size,
        received,
        state: record.state,
        accept_ranges: record.accept_ranges,
        connections: record.connections,
        speed_limit: record.speed_limit,
        category: record.category.clone(),
        priority: record.priority,
        error: record.error.clone(),
        sha256: record.sha256.clone(),
        created_at: record.created_at,
        updated_at: record.updated_at,
        finished_at: record.finished_at,
        speed,
        eta_secs,
        active_connections,
    }
}

async fn scheduler_task(weak: std::sync::Weak<Inner>) {
    loop {
        let (wake, shutdown) = match weak.upgrade() {
            Some(inner) => (inner.wake.clone(), inner.shutdown.clone()),
            None => break,
        };

        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = wake.notified() => {}
            _ = tokio::time::sleep(Duration::from_millis(750)) => {}
        }

        let Some(inner) = weak.upgrade() else {
            break;
        };

        let engine = Engine { inner };
        engine.pump_once().await;
    }
}

async fn monitor_progress(
    engine: Engine,
    id: Uuid,
    runtime: Arc<Runtime>,
    cancel: CancellationToken,
) {
    let mut last_bytes = runtime.progress.received.load(Ordering::SeqCst);
    let mut last_time = Instant::now();
    let mut smooth_speed = 0f64;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tokio::time::sleep(MONITOR_INTERVAL) => {}
        }

        if !runtime.is_active() {
            break;
        }

        let bytes = runtime.progress.received.load(Ordering::SeqCst);
        let now = Instant::now();
        let elapsed = now.duration_since(last_time).as_secs_f64();
        if elapsed > 0.0 {
            let instant = bytes.saturating_sub(last_bytes) as f64 / elapsed;
            smooth_speed = if smooth_speed == 0.0 {
                instant
            } else {
                smooth_speed * 0.55 + instant * 0.45
            };
            runtime
                .progress
                .speed
                .store(smooth_speed as u64, Ordering::SeqCst);
        }
        last_bytes = bytes;
        last_time = now;

        let total = runtime.progress.total.load(Ordering::SeqCst);
        let speed = runtime.progress.speed.load(Ordering::SeqCst);
        let eta_secs = if speed > 0 && total > bytes {
            Some((total - bytes) / speed)
        } else {
            None
        };

        let _ = engine.inner.events.send(EngineEvent::Progress {
            progress: ProgressEvent {
                id,
                received: bytes,
                total,
                speed,
                eta_secs,
                active_connections: runtime.progress.active_connections.load(Ordering::SeqCst),
            },
        });
    }
}

#[derive(Clone)]
struct SegmentedContext {
    db: Arc<Db>,
    client: Client,
    url: Url,
    headers: HashMap<String, String>,
    if_range: Option<String>,
    id: Uuid,
    part: PathBuf,
    retries: u32,
    token: CancellationToken,
    plan: Arc<Mutex<RangePlan>>,
    notify: Arc<Notify>,
    fatal: Arc<AtomicBool>,
    range_unsupported: Arc<AtomicBool>,
    error_slot: Arc<Mutex<Option<String>>>,
    progress: Arc<Progress>,
    limiter: Arc<RateLimiter>,
    global_limiter: Arc<RateLimiter>,
    persist: Arc<PersistThrottle>,
}

struct PersistThrottle {
    last: Mutex<Instant>,
    interval: Duration,
}

impl PersistThrottle {
    fn new(interval: Duration) -> Self {
        Self {
            last: Mutex::new(
                Instant::now()
                    .checked_sub(interval)
                    .unwrap_or_else(Instant::now),
            ),
            interval,
        }
    }

    fn due(&self) -> bool {
        let mut last = self.last.lock();
        if last.elapsed() >= self.interval {
            *last = Instant::now();
            true
        } else {
            false
        }
    }
}

enum BatchFailure {
    Retryable(String),
    Fatal(String),
    Aborted,
    RangeUnsupported,
}

async fn segmented_worker(ctx: SegmentedContext) {
    let mut file = match tokio::fs::OpenOptions::new()
        .write(true)
        .open(&ctx.part)
        .await
    {
        Ok(file) => file,
        Err(error) => {
            *ctx.error_slot.lock() = Some(format!("cannot open partial file: {error}"));
            ctx.fatal.store(true, Ordering::SeqCst);
            ctx.notify.notify_waiters();
            return;
        }
    };

    loop {
        if ctx.token.is_cancelled()
            || ctx.fatal.load(Ordering::SeqCst)
            || ctx.range_unsupported.load(Ordering::SeqCst)
        {
            return;
        }

        let job = ctx.plan.lock().acquire();
        let Some(job) = job else {
            let complete = ctx.plan.lock().is_complete();
            if complete {
                return;
            }
            tokio::select! {
                _ = ctx.token.cancelled() => return,
                _ = ctx.notify.notified() => continue,
                _ = tokio::time::sleep(Duration::from_millis(100)) => continue,
            }
        };

        ctx.progress
            .active_connections
            .fetch_add(1, Ordering::Relaxed);

        let outcome = download_batch(&ctx, &mut file, job).await;

        ctx.progress
            .active_connections
            .fetch_sub(1, Ordering::Relaxed);

        match outcome {
            Ok(()) => {
                ctx.plan.lock().commit(job, true);
                if ctx.persist.due() {
                    persist_plan(&ctx, false);
                }
                ctx.notify.notify_waiters();
            }
            Err(BatchFailure::Aborted) => {
                ctx.plan.lock().commit(job, false);
                ctx.notify.notify_waiters();
                return;
            }
            Err(BatchFailure::RangeUnsupported) => {
                ctx.plan.lock().commit(job, false);
                ctx.range_unsupported.store(true, Ordering::SeqCst);
                ctx.notify.notify_waiters();
                return;
            }
            Err(BatchFailure::Fatal(message)) => {
                ctx.plan.lock().commit(job, false);
                *ctx.error_slot.lock() = Some(message);
                ctx.fatal.store(true, Ordering::SeqCst);
                ctx.notify.notify_waiters();
                return;
            }
            Err(BatchFailure::Retryable(message)) => {
                ctx.plan.lock().commit(job, false);
                *ctx.error_slot.lock() = Some(message);
                ctx.fatal.store(true, Ordering::SeqCst);
                ctx.notify.notify_waiters();
                return;
            }
        }
    }
}

async fn download_batch(
    ctx: &SegmentedContext,
    file: &mut tokio::fs::File,
    job: BatchJob,
) -> std::result::Result<(), BatchFailure> {
    let mut attempts = 0u32;

    loop {
        if ctx.token.is_cancelled() {
            return Err(BatchFailure::Aborted);
        }

        match stream_batch_once(ctx, file, job).await {
            Ok(()) => return Ok(()),
            Err(BatchFailure::Retryable(message)) => {
                attempts += 1;
                if attempts > ctx.retries {
                    return Err(BatchFailure::Fatal(message));
                }
                sleep_backoff(attempts, &ctx.token)
                    .await
                    .map_err(|_| BatchFailure::Aborted)?;
            }
            Err(other) => return Err(other),
        }
    }
}

async fn stream_batch_once(
    ctx: &SegmentedContext,
    file: &mut tokio::fs::File,
    job: BatchJob,
) -> std::result::Result<(), BatchFailure> {
    let mut request = apply_headers(ctx.client.get(ctx.url.clone()), &ctx.headers).header(
        reqwest::header::RANGE,
        format!("bytes={}-{}", job.start, job.end.saturating_sub(1)),
    );
    if let Some(validator) = &ctx.if_range {
        request = request.header("If-Range", validator);
    }

    let response = tokio::select! {
        _ = ctx.token.cancelled() => return Err(BatchFailure::Aborted),
        result = request.send() => result,
    };

    let response = match response {
        Ok(response) => response,
        Err(error) => return Err(BatchFailure::Retryable(format!("network error: {error}"))),
    };

    let status = response.status();
    if status == StatusCode::OK {
        return Err(BatchFailure::RangeUnsupported);
    }
    if status == StatusCode::RANGE_NOT_SATISFIABLE {
        return Err(BatchFailure::Fatal(
            "server rejected the requested byte range".into(),
        ));
    }
    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        return Err(BatchFailure::Retryable(format!(
            "server responded with HTTP {status}"
        )));
    }
    if !status.is_success() {
        return Err(BatchFailure::Fatal(format!(
            "server responded with HTTP {status}"
        )));
    }

    if let Some(start) = content_range_start(&response) {
        if start != job.start {
            return Err(BatchFailure::Fatal(format!(
                "server returned range starting at {start}, expected {}",
                job.start
            )));
        }
    }

    if let Err(error) = file.seek(std::io::SeekFrom::Start(job.start)).await {
        return Err(BatchFailure::Retryable(format!(
            "cannot seek partial file: {error}"
        )));
    }

    let mut response = response;
    let mut written = 0u64;
    let mut buffer = Vec::with_capacity(WRITE_BUFFER);

    loop {
        let chunk = tokio::select! {
            _ = ctx.token.cancelled() => {
                ctx.progress.received.fetch_sub(written, Ordering::SeqCst);
                return Err(BatchFailure::Aborted);
            }
            chunk = response.chunk() => chunk,
        };

        match chunk {
            Ok(Some(bytes)) => {
                if bytes.is_empty() {
                    continue;
                }

                if written + bytes.len() as u64 > job.len() {
                    ctx.progress.received.fetch_sub(written, Ordering::SeqCst);
                    return Err(BatchFailure::Fatal(
                        "server sent more data than requested".into(),
                    ));
                }

                buffer.extend_from_slice(&bytes);
                written += bytes.len() as u64;
                ctx.progress
                    .received
                    .fetch_add(bytes.len() as u64, Ordering::SeqCst);

                ctx.global_limiter.acquire(bytes.len() as u64).await;
                ctx.limiter.acquire(bytes.len() as u64).await;

                if buffer.len() >= WRITE_BUFFER {
                    if let Err(error) = file.write_all(&buffer).await {
                        ctx.progress.received.fetch_sub(written, Ordering::SeqCst);
                        return Err(BatchFailure::Retryable(format!(
                            "cannot write to disk: {error}"
                        )));
                    }
                    buffer.clear();
                }

                if written >= job.len() {
                    break;
                }
            }
            Ok(None) => break,
            Err(error) => {
                ctx.progress.received.fetch_sub(written, Ordering::SeqCst);
                return Err(BatchFailure::Retryable(format!("network error: {error}")));
            }
        }
    }

    if written != job.len() {
        ctx.progress.received.fetch_sub(written, Ordering::SeqCst);
        return Err(BatchFailure::Retryable(format!(
            "short response ({} of {} bytes)",
            written,
            job.len()
        )));
    }

    if let Err(error) = file.write_all(&buffer).await {
        ctx.progress.received.fetch_sub(written, Ordering::SeqCst);
        return Err(BatchFailure::Retryable(format!(
            "cannot write to disk: {error}"
        )));
    }

    Ok(())
}

fn persist_plan(ctx: &SegmentedContext, force: bool) {
    if !force && !ctx.persist.due() {
        return;
    }
    let snapshot = ctx.plan.lock().snapshot();
    let received: u64 = snapshot
        .iter()
        .map(|(start, _end, pos)| pos.saturating_sub(*start))
        .sum();
    let _ = ctx.db.save_ranges(ctx.id, &snapshot);
    let _ = ctx.db.update_received(ctx.id, received);
}

fn plan_rows_valid(rows: &[(u64, u64, u64)], total: u64) -> bool {
    if rows.is_empty() {
        return false;
    }
    let mut next = 0u64;
    for (start, end, pos) in rows {
        if *start != next || *end <= *start || *pos < *start || *pos > *end {
            return false;
        }
        next = *end;
    }
    next == total
}

fn content_range_start(response: &Response) -> Option<u64> {
    let value = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)?
        .to_str()
        .ok()?;
    let rest = value.strip_prefix("bytes ")?;
    let range = rest.split('/').next()?;
    let start = range.split('-').next()?;
    start.parse().ok()
}

async fn prepare_part(path: &Path, total: u64, truncate: bool) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(truncate)
        .open(path)
        .await?;
    if total > 0 {
        file.set_len(total).await?;
    }
    Ok(())
}

async fn reset_for_single(
    db: &Arc<Db>,
    record: &DownloadRecord,
    part_path: &Path,
    total: u64,
) -> Result<()> {
    db.clear_ranges(record.id)?;
    db.update_received(record.id, 0)?;
    prepare_part(part_path, total, true).await?;
    Ok(())
}

async fn finalize_download(
    inner: &Arc<Inner>,
    record: &DownloadRecord,
    runtime: &Arc<Runtime>,
    part_path: &Path,
    total: u64,
    settings: &Settings,
) -> Result<()> {
    let metadata = tokio::fs::metadata(part_path).await?;
    let received = metadata.len();
    if total > 0 && received != total {
        return Err(Error::failed(format!(
            "download incomplete ({received}/{total} bytes)"
        )));
    }

    if settings.durable_finish {
        let file = tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(part_path)
            .await?;
        file.sync_all().await?;
        drop(file);
    }

    inner
        .db
        .set_state(record.id, DownloadState::Completing, None)?;

    let sha256 = if settings.compute_sha256 {
        Some(sha256_file(part_path).await?)
    } else {
        None
    };

    let mut destination = Path::new(&record.dir).join(&record.filename);
    if destination.exists() {
        destination = unique_destination(Path::new(&record.dir), &record.filename);
    }
    tokio::fs::rename(part_path, &destination).await?;
    let filename = destination
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| record.filename.clone());

    let final_size = if total == 0 { received } else { total };
    runtime
        .progress
        .received
        .store(final_size, Ordering::SeqCst);
    runtime.progress.total.store(final_size, Ordering::SeqCst);

    inner.db.update_meta(
        record.id,
        None,
        final_size,
        record.accept_ranges,
        None,
        None,
        None,
    )?;
    inner.db.finish(
        record.id,
        DownloadState::Done,
        final_size,
        sha256.as_deref(),
        &filename,
        &record.dir,
    )?;
    inner.db.clear_ranges(record.id)?;

    Ok(())
}

async fn sha256_file(path: &Path) -> Result<String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<String> {
        use std::io::Read;
        let mut file = std::fs::File::open(&path)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 1 << 20];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok(hex::encode(hasher.finalize()))
    })
    .await
    .map_err(|error| Error::Other(error.to_string()))?
}

fn backoff_delay(attempt: u32) -> Duration {
    let exponent = attempt.min(5);
    let base = 300u64.saturating_mul(1u64 << exponent).min(8_000);
    let jitter = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| (duration.subsec_nanos() as u64) % 250)
        .unwrap_or(0);
    Duration::from_millis(base + jitter)
}

async fn sleep_backoff(attempt: u32, token: &CancellationToken) -> Result<()> {
    let delay = backoff_delay(attempt);
    tokio::select! {
        _ = token.cancelled() => Err(Error::Cancelled),
        _ = tokio::time::sleep(delay) => Ok(()),
    }
}
