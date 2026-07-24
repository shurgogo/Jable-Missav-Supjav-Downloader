use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskControlState {
    Running,
    Paused,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct TaskControlInfo {
    pub state: TaskControlState,
    pub title: String,
    pub save_dir: String,
    pub max_concurrent: usize,
    pub resolution: String,
}

pub type TaskRegistry = Arc<Mutex<HashMap<String, TaskControlInfo>>>;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProgressPayload {
    pub url: String,
    pub title: String,
    pub index: usize,
    pub total: usize,
    pub speed_kbps: f64,
    pub status: String, // "downloading" | "merging" | "completed" | "failed" | "paused"
}
