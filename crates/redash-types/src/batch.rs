use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskState {
    Pending,
    Running,
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HostTaskExecution {
    pub host_id: String,
    pub host_name: String,
    pub state: TaskState,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<u32>,
    pub duration_ms: u64,
    #[serde(default)]
    pub duration_us: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BatchJobResult {
    pub job_id: String,
    pub command: String,
    pub hosts_results: HashMap<String, HostTaskExecution>,
    pub total_duration_ms: u64,
    #[serde(default)]
    pub total_duration_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchRunRequest {
    pub host_ids: Vec<String>,
    pub command: String,
}
