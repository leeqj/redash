use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::config::HostConfig;
use crate::session::SessionManager;

pub use redash_types::batch::{BatchJobResult, BatchRunRequest, HostTaskExecution, TaskState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BatchProgressEvent {
    HostStarted { host_id: String, host_name: String },
    HostCompleted(HostTaskExecution),
    AllCompleted(BatchJobResult),
}

pub struct BatchRunner;

impl BatchRunner {
    pub async fn run_batch(
        hosts: Vec<HostConfig>,
        command: String,
        session_mgr: Arc<SessionManager>,
        timeout_dur: Duration,
    ) -> BatchJobResult {
        Self::run_batch_streaming(hosts, command, session_mgr, timeout_dur, None).await
    }

    pub async fn run_batch_streaming(
        hosts: Vec<HostConfig>,
        command: String,
        session_mgr: Arc<SessionManager>,
        timeout_dur: Duration,
        progress_tx: Option<tokio::sync::mpsc::UnboundedSender<BatchProgressEvent>>,
    ) -> BatchJobResult {
        let job_id = Uuid::new_v4().to_string();
        let start = std::time::Instant::now();

        let mut handles = tokio::task::JoinSet::new();

        for host in hosts {
            let session_mgr = Arc::clone(&session_mgr);
            let cmd = command.clone();
            let tx_opt = progress_tx.clone();

            handles.spawn(async move {
                let host_id = host.id.0.clone();
                let host_name = host.name.clone();

                if let Some(ref tx) = tx_opt {
                    let _ = tx.send(BatchProgressEvent::HostStarted {
                        host_id: host_id.clone(),
                        host_name: host_name.clone(),
                    });
                }

                let host_start = std::time::Instant::now();
                let exec = match session_mgr.exec(&host, &cmd, timeout_dur).await {
                    Ok(exec_res) => {
                        let host_elapsed = host_start.elapsed();
                        let state = if exec_res.exit_code == 0 {
                            TaskState::Success
                        } else {
                            TaskState::Failed
                        };
                        HostTaskExecution {
                            host_id,
                            host_name,
                            state,
                            stdout: exec_res.stdout,
                            stderr: exec_res.stderr,
                            exit_code: Some(exec_res.exit_code),
                            duration_ms: host_elapsed.as_millis() as u64,
                            duration_us: host_elapsed.as_micros() as u64,
                            error: None,
                        }
                    }
                    Err(e) => {
                        let host_elapsed = host_start.elapsed();
                        HostTaskExecution {
                            host_id,
                            host_name,
                            state: TaskState::Failed,
                            stdout: String::new(),
                            stderr: String::new(),
                            exit_code: None,
                            duration_ms: host_elapsed.as_millis() as u64,
                            duration_us: host_elapsed.as_micros() as u64,
                            error: Some(format!("{e:#}")),
                        }
                    }
                };

                if let Some(ref tx) = tx_opt {
                    let _ = tx.send(BatchProgressEvent::HostCompleted(exec.clone()));
                }

                exec
            });
        }

        let mut hosts_results = HashMap::new();
        while let Some(result) = handles.join_next().await {
            if let Ok(res) = result {
                hosts_results.insert(res.host_id.clone(), res);
            }
        }

        let total_elapsed = start.elapsed();
        let final_job = BatchJobResult {
            job_id,
            command,
            hosts_results,
            total_duration_ms: total_elapsed.as_millis() as u64,
            total_duration_us: total_elapsed.as_micros() as u64,
        };

        if let Some(ref tx) = progress_tx {
            let _ = tx.send(BatchProgressEvent::AllCompleted(final_job.clone()));
        }

        final_job
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_job_result_serialization() {
        let mut results = HashMap::new();
        let hid = "node-1".to_string();
        results.insert(
            hid.clone(),
            HostTaskExecution {
                host_id: hid.clone(),
                host_name: "prod-web".into(),
                state: TaskState::Success,
                stdout: "hello world\n".into(),
                stderr: String::new(),
                exit_code: Some(0),
                duration_ms: 120,
                duration_us: 120_500,
                error: None,
            },
        );

        let job = BatchJobResult {
            job_id: "test-job-uuid".into(),
            command: "echo 'hello world'".into(),
            hosts_results: results,
            total_duration_ms: 125,
            total_duration_us: 125_450,
        };

        let json = serde_json::to_string(&job).expect("serialize should succeed");
        let deserialized: BatchJobResult =
            serde_json::from_str(&json).expect("deserialize should succeed");

        assert_eq!(deserialized.job_id, "test-job-uuid");
        let res = deserialized.hosts_results.get(&hid).unwrap();
        assert_eq!(res.state, TaskState::Success);
        assert_eq!(res.exit_code, Some(0));
        assert_eq!(res.duration_ms, 120);
        assert_eq!(res.duration_us, 120_500);
        assert_eq!(deserialized.total_duration_us, 125_450);
    }

    #[test]
    fn test_batch_progress_event_serialization() {
        let hid = "node-2".to_string();
        let event_started = BatchProgressEvent::HostStarted {
            host_id: hid.clone(),
            host_name: "db-node".into(),
        };
        let json_started = serde_json::to_string(&event_started).expect("serialize started event");
        let de_started: BatchProgressEvent =
            serde_json::from_str(&json_started).expect("deserialize started event");
        match de_started {
            BatchProgressEvent::HostStarted { host_id, host_name } => {
                assert_eq!(host_id, hid);
                assert_eq!(host_name, "db-node");
            }
            _ => panic!("Expected HostStarted variant"),
        }

        let task_exec = HostTaskExecution {
            host_id: hid.clone(),
            host_name: "db-node".into(),
            state: TaskState::Failed,
            stdout: String::new(),
            stderr: "connection reset".into(),
            exit_code: Some(255),
            duration_ms: 35,
            duration_us: 35_100,
            error: Some("network error".into()),
        };
        let event_completed = BatchProgressEvent::HostCompleted(task_exec);
        let json_completed =
            serde_json::to_string(&event_completed).expect("serialize completed event");
        let de_completed: BatchProgressEvent =
            serde_json::from_str(&json_completed).expect("deserialize completed event");
        match de_completed {
            BatchProgressEvent::HostCompleted(exec) => {
                assert_eq!(exec.state, TaskState::Failed);
                assert_eq!(exec.exit_code, Some(255));
                assert_eq!(exec.duration_us, 35_100);
            }
            _ => panic!("Expected HostCompleted variant"),
        }
    }

    #[test]
    fn test_task_state_equality() {
        assert_eq!(TaskState::Pending, TaskState::Pending);
        assert_eq!(TaskState::Running, TaskState::Running);
        assert_eq!(TaskState::Success, TaskState::Success);
        assert_eq!(TaskState::Failed, TaskState::Failed);
        assert_ne!(TaskState::Pending, TaskState::Running);
        assert_ne!(TaskState::Success, TaskState::Failed);
    }
}
