//! HyperFetch core: a modular, resumable, multi-connection download engine.
//!
//! The crate is GUI-agnostic: the Tauri application, the CLI harness and the
//! test-suite all drive the same [`Engine`].

pub mod db;
pub mod engine;
pub mod error;
pub mod grabber;
pub mod plan;
pub mod probe;
pub mod ratelimit;
pub mod types;
pub mod util;

pub use engine::{Engine, EngineEvent, EngineStats};
pub use error::{Error, Result};
pub use probe::{probe_url, ProbeResult};
pub use types::{
    Category, DownloadId, DownloadSnapshot, DownloadSpec, DownloadState, ProgressEvent, Settings,
};
