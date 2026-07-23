use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppError {
    pub code: String,
    pub params: Option<Value>,
}

impl AppError {
    pub fn new(code: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            code: code.into(),
            params,
        }
    }

    pub fn simple(code: impl Into<String>) -> Self {
        Self::new(code, None)
    }

    pub fn with_param(code: impl Into<String>, key: &str, val: impl Serialize) -> Self {
        let mut map = serde_json::Map::new();
        if let Ok(json_val) = serde_json::to_value(val) {
            map.insert(key.to_string(), json_val);
        }
        Self::new(code, Some(Value::Object(map)))
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Ok(json_str) = serde_json::to_string(self) {
            write!(f, "{}", json_str)
        } else {
            write!(f, r#"{{"code":"{}"}}"#, self.code)
        }
    }
}

impl std::error::Error for AppError {}

impl From<AppError> for String {
    fn from(err: AppError) -> Self {
        err.to_string()
    }
}

pub fn map_scraper_error(err_str: impl Into<String>, url: &str) -> String {
    let s = err_str.into();
    if s.contains("403") || s.to_lowercase().contains("forbidden") || s.to_lowercase().contains("cloudflare") {
        let domain = url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .unwrap_or_else(|| url.to_string());
        AppError::with_param("CF_VERIFICATION_REQUIRED", "domain", domain).to_string()
    } else {
        AppError::with_param("NETWORK_CONNECT_FAILED", "url", url).to_string()
    }
}
