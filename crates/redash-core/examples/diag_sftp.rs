use redash_core::config::HostStore;
use redash_core::session::SessionManager;
use redash_core::sftp::SftpManager;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let store_path = HostStore::default_path();
    println!("Loading host store from {:?}", store_path);
    let store = HostStore::load_from_file(&store_path)?;
    println!("Found {} hosts", store.hosts.len());

    let session_mgr = Arc::new(SessionManager::new());

    for host in &store.hosts {
        println!("\n==========================================");
        println!(
            "Testing Host: {} ({}@{}:{})",
            host.name, host.user, host.hostname, host.port
        );
        println!("Auth method: {:?}", host.auth);

        println!("Testing concurrent probe exec and SFTP operations...");
        let host_for_probe = host.clone();
        let mgr_for_probe = Arc::clone(&session_mgr);
        let probe_task = tokio::spawn(async move {
            for i in 0..10 {
                let cmd = "uname -a && uptime && free -m 2>/dev/null || top -l 1 | head -n 10";
                let res = mgr_for_probe
                    .exec(&host_for_probe, cmd, std::time::Duration::from_secs(5))
                    .await;
                println!("  [Probe #{}] exec success: {}", i, res.is_ok());
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        });

        match session_mgr.open_sftp(host).await {
            Ok(sftp) => {
                println!("SUCCESS: SFTP session opened!");

                let home = SftpManager::resolve_initial_dir(&sftp, "/").await;
                println!("resolve_initial_dir = {:?}", home);

                for round in 1..=5 {
                    println!("Testing list_dir on {:?} (Round {})...", home, round);
                    match SftpManager::list_dir(&sftp, &home).await {
                        Ok(items) => {
                            println!(
                                "SUCCESS (Round {}): list_dir returned {} items",
                                round,
                                items.len()
                            );
                        }
                        Err(e) => {
                            println!(
                                "ERROR (Round {}): list_dir on {:?} failed: {:?}",
                                round, home, e
                            );
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                }
            }
            Err(e) => {
                println!("FAILED to open_sftp for {}: {:?}", host.name, e);
            }
        }
        let _ = probe_task.await;
    }

    Ok(())
}
