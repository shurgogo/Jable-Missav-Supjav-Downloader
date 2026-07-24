use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub mod cf;
pub mod downloader;
pub mod media;
pub mod scraper;
pub mod system;

pub use cf::*;
pub use downloader::*;
pub use media::*;
pub use scraper::*;
pub use system::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CfConfig {
    pub cf_clearance: String,
    pub user_agent: String,
}

pub struct AppState {
    pub client: wreq::Client,
    pub task_states: crate::downloader::TaskRegistry,
    pub cf_configs: Arc<Mutex<HashMap<String, CfConfig>>>,
}
