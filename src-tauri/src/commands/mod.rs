use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

pub mod cf;
pub mod downloader;
pub mod media;
pub mod proxy;
pub mod scraper;
pub mod system;
pub mod updater;

pub use cf::*;
pub use downloader::*;
pub use media::*;
pub use proxy::*;
pub use scraper::*;
pub use system::*;
pub use updater::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CfConfig {
    pub cf_clearance: String,
    pub user_agent: String,
}

pub struct AppState {
    /// Shared HTTP client. Wrapped in a `Mutex` so proxy settings can swap in
    /// a rebuilt client at runtime; commands clone it out before awaiting.
    pub client: Mutex<wreq::Client>,
    pub task_states: crate::downloader::TaskRegistry,
    pub cf_configs: Arc<Mutex<HashMap<String, CfConfig>>>,
    /// Monotonic counter assigning a unique generation to each download
    /// task instance (see `TaskControlInfo::generation`).
    pub task_generation: Arc<AtomicU64>,
    /// Effective proxy status, as last applied (None before first apply).
    pub proxy_status: Arc<Mutex<Option<crate::proxy::ProxyStatus>>>,
}
