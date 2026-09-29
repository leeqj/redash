use log::{debug, warn};
use redash_types::ContainerSummary;
use serde::Deserialize;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

const DOCKER_SOCK_PATH: &str = "/var/run/docker.sock";

#[derive(Debug, Deserialize)]
struct DockerContainerRaw {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Names", default)]
    names: Vec<String>,
    #[serde(rename = "Image")]
    image: String,
    #[serde(rename = "State")]
    state: String,
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "Created")]
    created: i64,
    #[serde(rename = "Ports", default)]
    ports: Vec<DockerPortRaw>,
}

#[derive(Debug, Deserialize)]
struct DockerPortRaw {
    #[serde(rename = "PublicPort")]
    public_port: Option<u16>,
    #[serde(rename = "PrivatePort")]
    private_port: u16,
    #[serde(rename = "Type")]
    port_type: String,
}

pub struct DockerClient {
    socket_path: String,
}

impl Default for DockerClient {
    fn default() -> Self {
        Self::new()
    }
}

impl DockerClient {
    pub fn new() -> Self {
        Self {
            socket_path: DOCKER_SOCK_PATH.to_string(),
        }
    }

    pub fn is_available(&self) -> bool {
        Path::new(&self.socket_path).exists()
    }

    /// Fetches all containers (running and stopped) via the Docker engine unix socket.
    pub async fn list_containers(&self) -> Vec<ContainerSummary> {
        if !self.is_available() {
            return Vec::new();
        }

        let response = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.raw_http_get("/containers/json?all=1"),
        )
        .await
        .unwrap_or_else(|_| Err("Docker collection timed out after 2s".into()));
        match response {
            Ok(body) => match serde_json::from_slice::<Vec<DockerContainerRaw>>(&body) {
                Ok(raw_list) => raw_list
                    .into_iter()
                    .map(|c| {
                        let name = c
                            .names
                            .first()
                            .map(|n| n.trim_start_matches('/').to_string())
                            .unwrap_or_else(|| c.id[..12.min(c.id.len())].to_string());

                        let ports = c
                            .ports
                            .into_iter()
                            .map(|p| {
                                if let Some(pub_p) = p.public_port {
                                    format!(
                                        "{}:{}->{}/{}",
                                        pub_p, p.private_port, p.private_port, p.port_type
                                    )
                                } else {
                                    format!("{}/{}", p.private_port, p.port_type)
                                }
                            })
                            .collect();

                        ContainerSummary {
                            id: c.id[..12.min(c.id.len())].to_string(),
                            name,
                            image: c.image,
                            state: c.state,
                            status: c.status,
                            created: c.created,
                            ports,
                        }
                    })
                    .collect(),
                Err(e) => {
                    warn!(
                        "Failed to deserialize docker /containers/json response: {}",
                        e
                    );
                    Vec::new()
                }
            },
            Err(e) => {
                debug!("Failed to query docker socket: {}", e);
                Vec::new()
            }
        }
    }

    /// Restarts a container by ID or Name.
    pub async fn restart_container(&self, id: &str) -> Result<String, String> {
        let path = format!("/containers/{}/restart", id);
        self.raw_http_post(&path, "").await
    }

    /// Stops a container by ID or Name.
    pub async fn stop_container(&self, id: &str) -> Result<String, String> {
        let path = format!("/containers/{}/stop", id);
        self.raw_http_post(&path, "").await
    }

    /// Prunes unused containers.
    pub async fn prune_containers(&self) -> Result<String, String> {
        self.raw_http_post("/containers/prune", "").await
    }

    async fn raw_http_get(&self, path: &str) -> Result<Vec<u8>, String> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(|e| format!("Cannot connect to docker socket: {}", e))?;

        let request = format!(
            "GET {} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            path
        );

        stream
            .write_all(request.as_bytes())
            .await
            .map_err(|e| format!("Write failed: {}", e))?;

        let mut buffer = Vec::new();
        stream
            .read_to_end(&mut buffer)
            .await
            .map_err(|e| format!("Read failed: {}", e))?;

        extract_http_body(&buffer)
    }

    async fn raw_http_post(&self, path: &str, body: &str) -> Result<String, String> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .await
            .map_err(|e| format!("Cannot connect to docker socket: {}", e))?;

        let request = format!(
            "POST {} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            path,
            body.len(),
            body
        );

        stream
            .write_all(request.as_bytes())
            .await
            .map_err(|e| format!("Write failed: {}", e))?;

        let mut buffer = Vec::new();
        stream
            .read_to_end(&mut buffer)
            .await
            .map_err(|e| format!("Read failed: {}", e))?;

        let res_bytes = extract_http_body(&buffer)?;
        Ok(String::from_utf8_lossy(&res_bytes).to_string())
    }
}

fn extract_http_body(buffer: &[u8]) -> Result<Vec<u8>, String> {
    if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
        let header_bytes = &buffer[..pos];
        let body = &buffer[pos + 4..];

        let header_str = String::from_utf8_lossy(header_bytes);
        if let Some(first_line) = header_str.lines().next()
            && !first_line.contains("200")
            && !first_line.contains("204")
            && !first_line.contains("201")
        {
            return Err(format!("Docker API returned status: {}", first_line));
        }

        let is_chunked = header_str.lines().any(|line| {
            let lower = line.to_ascii_lowercase();
            lower.starts_with("transfer-encoding:") && lower.contains("chunked")
        });

        if is_chunked {
            decode_chunked_body(body)
        } else {
            Ok(body.to_vec())
        }
    } else {
        Err("Malformed HTTP response from docker socket".to_string())
    }
}

pub fn decode_chunked_body(mut body: &[u8]) -> Result<Vec<u8>, String> {
    let mut decoded = Vec::new();
    loop {
        if body.is_empty() {
            break;
        }
        let pos = body
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| "Invalid chunked body: missing CRLF after chunk size".to_string())?;

        let size_str = std::str::from_utf8(&body[..pos])
            .map_err(|e| format!("Invalid utf8 in chunk size: {}", e))?
            .trim();
        let size_str = size_str.split(';').next().unwrap_or(size_str).trim();
        let chunk_size = usize::from_str_radix(size_str, 16)
            .map_err(|e| format!("Invalid hex chunk size '{}': {}", size_str, e))?;

        if chunk_size == 0 {
            break;
        }

        let data_start = pos + 2;
        let data_end = data_start + chunk_size;
        if body.len() < data_end {
            return Err("Incomplete chunk data in HTTP body".to_string());
        }

        decoded.extend_from_slice(&body[data_start..data_end]);

        if body.len() >= data_end + 2 && &body[data_end..data_end + 2] == b"\r\n" {
            body = &body[data_end + 2..];
        } else if body.len() > data_end && body[data_end] == b'\n' {
            body = &body[data_end + 1..];
        } else {
            body = &body[data_end..];
        }
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stalled_docker_daemon_does_not_stall_telemetry() {
        let path = std::path::PathBuf::from("/tmp")
            .join(format!("redash-docker-{}.sock", uuid::Uuid::new_v4()));
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let daemon = tokio::spawn(async move {
            let (_socket, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        let client = DockerClient {
            socket_path: path.to_string_lossy().into_owned(),
        };
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(3), client.list_containers()).await;
        daemon.abort();
        let _ = daemon.await;
        std::fs::remove_file(path).unwrap();
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_decode_chunked_body() {
        let chunked_raw = b"4\r\nWiki\r\n5\r\npedia\r\nf\r\n in \r\n\r\nchunks.\r\n0\r\n\r\n";
        let decoded = decode_chunked_body(chunked_raw).unwrap();
        assert_eq!(
            String::from_utf8(decoded).unwrap(),
            "Wikipedia in \r\n\r\nchunks."
        );
    }

    #[test]
    fn test_extract_http_body_chunked() {
        let payload = b"[{\"Id\":\"container-1\"}]";
        let hex_len = format!("{:x}", payload.len());
        let http_response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n{}\r\n[{{\"Id\":\"container-1\"}}]\r\n0\r\n\r\n",
            hex_len
        );
        let body = extract_http_body(http_response.as_bytes()).unwrap();
        assert_eq!(
            String::from_utf8(body).unwrap(),
            "[{\"Id\":\"container-1\"}]"
        );
    }

    #[test]
    fn test_extract_http_body_plain() {
        let http_response = b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\nHello, World!";
        let body = extract_http_body(http_response).unwrap();
        assert_eq!(String::from_utf8(body).unwrap(), "Hello, World!");
    }
}
