pub mod batch;
pub mod hosts;
pub mod settings;
pub mod sftp;

use axum::Router;
use axum::routing::{get, post};

use crate::state::AppState;

pub fn api_router() -> Router<AppState> {
    Router::new()
        // Host management
        .route("/api/hosts", get(hosts::list_hosts).post(hosts::save_host))
        .route(
            "/api/hosts/{id}",
            get(hosts::get_host).delete(hosts::delete_host),
        )
        .route("/api/hosts/{id}/test", post(hosts::test_connection))
        .route("/api/hosts/test", post(hosts::test_draft_connection))
        .route(
            "/api/settings",
            get(settings::get_settings)
                .post(settings::save_settings)
                .put(settings::save_settings),
        )
        .route("/api/settings/test-webhook", post(settings::test_webhook))
        // SFTP management
        .route("/api/sftp/{host_id}/list", get(sftp::list_directory))
        .route("/api/sftp/{host_id}/read", get(sftp::read_file_content))
        .route("/api/sftp/{host_id}/write", post(sftp::write_file_content))
        // Batch execution
        .route("/api/batch/exec", post(batch::run_batch_job))
}
