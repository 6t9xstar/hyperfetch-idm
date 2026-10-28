//! Link, media and site grabbers.
//!
//! The grabbers are intentionally free of UI concerns: they inspect pages and
//! manifests, resolve URLs and hand the results to the download engine.

pub mod hls;
pub mod site;
