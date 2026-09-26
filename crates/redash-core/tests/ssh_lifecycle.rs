use redash_core::{
    batch::{BatchProgressEvent, BatchRunner},
    config::HostConfig,
    session::{
        client::ClientHandler,
        exec::ExecChannel,
        tunnel::{TunnelConfig, TunnelManager, TunnelType},
    },
};
use russh::{
    Channel, ChannelId,
    server::{Auth, ChannelOpenHandle, Msg, Session},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

struct Server {
    channels: HashMap<ChannelId, Channel<Msg>>,
    closed: Arc<AtomicUsize>,
}
impl russh::server::Handler for Server {
    type Error = russh::Error;
    async fn auth_none(&mut self, _: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }
    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }
    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if data == b"reject" {
            session.channel_failure(channel)?;
            session.close(channel)?;
            return Ok(());
        }
        session.channel_success(channel)?;
        match data {
            b"signal" => {
                session.exit_signal_request(
                    channel,
                    russh::Sig::TERM,
                    false,
                    "test signal",
                    "en",
                )?;
                session.close(channel)?;
            }
            b"missing" => session.close(channel)?,
            b"success" => {
                session.data(channel, b"ok".to_vec())?;
                session.exit_status_request(channel, 0)?;
                session.eof(channel)?;
                session.close(channel)?;
            }
            _ => {}
        }
        Ok(())
    }
    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.remove(&channel);
        self.closed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct TestConnection {
    client: Arc<russh::client::Handle<ClientHandler>>,
    closed: Arc<AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
    directory: PathBuf,
}
impl Drop for TestConnection {
    fn drop(&mut self) {
        self.server.abort();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
async fn connect() -> TestConnection {
    let key =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519).unwrap();
    let directory = std::env::temp_dir().join(format!("redash-ssh-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let known_hosts = directory.join("known_hosts");
    russh::keys::known_hosts::learn_known_hosts_path(
        "memory.test",
        22,
        key.public_key(),
        &known_hosts,
    )
    .unwrap();
    let config = russh::server::Config {
        keys: vec![key],
        ..Default::default()
    };
    let closed = Arc::new(AtomicUsize::new(0));
    let handler = Server {
        channels: HashMap::new(),
        closed: Arc::clone(&closed),
    };
    let (client, server) = tokio::io::duplex(65536);
    let server = tokio::spawn(async move {
        let _ = russh::server::run_stream(Arc::new(config), server, handler)
            .await
            .unwrap()
            .await;
    });
    let mut client = russh::client::connect_stream(
        Arc::new(russh::client::Config::default()),
        client,
        ClientHandler::with_known_hosts("memory.test", 22, known_hosts),
    )
    .await
    .unwrap();
    assert!(client.authenticate_none("test").await.unwrap().success());
    TestConnection {
        client: Arc::new(client),
        closed,
        server,
        directory,
    }
}

#[tokio::test]
async fn host_keys_must_be_known_and_unchanged() {
    use russh::client::Handler;
    let directory =
        std::env::temp_dir().join(format!("redash-known-hosts-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("known_hosts");
    let key =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519).unwrap();
    let public = russh::keys::PublicKeyOrCertificate::from(key.public_key().clone());
    let mut handler = ClientHandler::with_known_hosts("example.test", 2222, path.clone());
    assert!(handler.check_server_key(&public).await.is_err());
    russh::keys::known_hosts::learn_known_hosts_path("example.test", 2222, key.public_key(), &path)
        .unwrap();
    assert!(handler.check_server_key(&public).await.unwrap());
    let changed =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::Algorithm::Ed25519).unwrap();
    assert!(
        handler
            .check_server_key(&changed.public_key().clone().into())
            .await
            .is_err()
    );
    let mut contents = std::fs::read_to_string(&path).unwrap();
    contents.push_str(&format!(
        "@revoked example.test {}\n",
        key.public_key().to_openssh().unwrap()
    ));
    std::fs::write(&path, contents).unwrap();
    assert!(
        handler
            .check_server_key(&public)
            .await
            .unwrap_err()
            .to_string()
            .contains("revoked")
    );
    let mut wrong_port = ClientHandler::with_known_hosts("example.test", 22, path);
    assert!(wrong_port.check_server_key(&public).await.is_err());
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn command_outcomes_require_a_real_exit_status() {
    let connection = connect().await;
    for command in ["signal", "missing", "reject"] {
        assert!(
            ExecChannel::execute(&connection.client, command, Duration::from_secs(2))
                .await
                .is_err(),
            "{command}"
        );
    }
    let result = ExecChannel::execute(&connection.client, "success", Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout, "ok");
}

async fn wait_for_close(closed: &AtomicUsize, previous: usize) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while closed.load(Ordering::SeqCst) <= previous {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("SSH peer must receive channel-close");
}
#[tokio::test]
async fn timeout_and_abort_close_the_remote_channel() {
    let connection = connect().await;
    assert!(
        ExecChannel::execute(&connection.client, "wait", Duration::from_millis(80))
            .await
            .is_err()
    );
    wait_for_close(&connection.closed, 0).await;
    let previous = connection.closed.load(Ordering::SeqCst);
    let client = Arc::clone(&connection.client);
    let task = tokio::spawn(async move {
        ExecChannel::execute(&client, "wait", Duration::from_secs(60)).await
    });
    tokio::time::sleep(Duration::from_millis(80)).await;
    task.abort();
    let _ = task.await;
    wait_for_close(&connection.closed, previous).await;
}

#[tokio::test]
async fn aborting_a_batch_drops_its_children_and_progress_senders() {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let mut host = HostConfig::new("stalled", "127.0.0.1", "test");
    host.port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
        std::future::pending::<()>().await;
    });
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let runner = tokio::spawn(BatchRunner::run_batch_streaming(
        vec![host],
        "wait".into(),
        Arc::new(redash_core::session::SessionManager::new()),
        Duration::from_secs(60),
        Some(tx),
    ));
    assert!(matches!(
        rx.recv().await,
        Some(BatchProgressEvent::HostStarted { .. })
    ));
    runner.abort();
    let _ = runner.await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while rx.recv().await.is_some() {}
    })
    .await
    .expect("No detached batch children");
    server.abort();
}

#[tokio::test]
async fn stopping_and_dropping_tunnels_release_listeners() {
    let connection = connect().await;
    let manager = Arc::new(TunnelManager::new());
    let config = TunnelConfig {
        id: "test".into(),
        name: "test".into(),
        active: false,
        tunnel_type: TunnelType::DynamicSocks5 { local_port: 0 },
    };
    manager
        .start_tunnel(config.clone(), Arc::clone(&connection.client))
        .await
        .unwrap();
    let port = manager.list_active().await[0].tunnel_type.local_port();
    let reader = Arc::clone(&manager);
    let read_task = tokio::spawn(async move {
        for _ in 0..100 {
            reader.list_tunnel_stats().await;
            tokio::task::yield_now().await;
        }
    });
    tokio::time::timeout(Duration::from_secs(3), manager.stop_tunnel("test"))
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), read_task)
        .await
        .unwrap()
        .unwrap();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .unwrap();
    drop(listener);
    manager
        .start_tunnel(config, Arc::clone(&connection.client))
        .await
        .unwrap();
    let port = manager.list_active().await[0].tunnel_type.local_port();
    drop(manager);
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(listener) = tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
                drop(listener);
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("Dropping the workbench's manager releases the listener");
}
