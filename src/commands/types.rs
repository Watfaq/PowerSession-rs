use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub(crate) enum RecordHeader {
    V2(RecordHeaderV2),
    V3(RecordHeaderV3),
}

#[derive(Serialize, Deserialize)]
pub(crate) struct RecordHeaderV2 {
    pub(crate) version: u8,
    pub(crate) width: i16,
    pub(crate) height: i16,
    pub(crate) timestamp: u64,
    #[serde(rename = "env")]
    pub(crate) environment: HashMap<String, String>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct RecordHeaderV3Term {
    #[serde(rename = "cols")]
    pub(crate) width: i16,
    #[serde(rename = "rows")]
    pub(crate) height: i16,
    #[serde(rename = "type")]
    pub(crate) terminal_type: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) theme: Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct RecordHeaderV3 {
    pub(crate) version: u8,
    pub(crate) timestamp: Option<u64>,
    pub(crate) term: RecordHeaderV3Term,
    pub(crate) title: Option<String>,
    #[serde(rename = "env")]
    pub(crate) environment: HashMap<String, String>,
    pub(crate) command: Option<String>,
    pub(crate) idle_time_limit: Option<f64>,
    pub(crate) tags: Option<Vec<String>>,
}

/// Represents an asciinema v1 format recording (entire file is one JSON object).
/// The `stdout` field contains `[delay_seconds, text]` pairs with relative timing.
#[derive(Deserialize)]
pub(crate) struct V1Recording {
    pub(crate) version: u8,
    pub(crate) width: i16,
    pub(crate) height: i16,
    #[serde(default)]
    pub(crate) stdout: Vec<(f64, String)>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct SessionLine {
    pub(crate) timestamp: f64,
    pub(crate) stdout: bool,
    pub(crate) content: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum LineItem {
    String(String),
    F64(f64),
}
