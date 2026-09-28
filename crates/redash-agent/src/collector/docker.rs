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

        match self.raw_http_get("/containers/json?all=1").await {
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
                                    format!("{}:{}->{}/{}", pub_p, p.private_port, p.private_port, p.port_type)
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
                    warn!("Failed to deserialize docker /containers/json response: {}", e);
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
        if let Some(first_line) = header_str.lines().next() {
            if first_line.contains("200") || first_line.contains("204") || first_line.contains("201") {
                return Ok(body.to_vec());
            } else {
                return Err(format!("Docker API returned status: {}", first_line));
            }
        }
        Ok(body.to_vec())
    } else {
        Err("Malformed HTTP response from docker socket".to_string())
    }
}
