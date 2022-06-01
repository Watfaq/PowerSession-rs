use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub(crate) struct RecordHeader {
    pub(crate) version: u8,
    pub(crate) width: i16,
    pub(crate) height: i16,
    pub(crate) timestamp: u64,
    #[serde(rename = "env")]
    pub(crate) environment: HashMap<String, String>,
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
