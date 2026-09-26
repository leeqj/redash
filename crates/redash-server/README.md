# ReDash Server & Web Gateway

`redash-server` is the headless Web Gateway and HTTP/WebSocket server for ReDash. It enables running ReDash completely in modern web browsers while maintaining high performance, raw SSH streaming, real-time node telemetry, and AI Agent detection.

---

## 🏗️ Architecture

```
┌────────────────────────────────────────────────────────┐
│               Modern Browser (Web Client)              │
│  - Cyberpunk DarkTech Theme                            │
│  - Interactive Web SSH Terminal                        │
│  - Real-time AI Agent HUD (Claude, Aider, Kimi, etc.)  │
│  - SFTP File Tree & Editor                             │
│  - Fleet Grid with Live CPU/MEM Gauges                 │
└──────────────────────────┬─────────────────────────────┘
                           │ HTTP REST / WebSocket
                           ▼
┌────────────────────────────────────────────────────────┐
│           redash-server (Axum + Tokio Gateway)         │
│  - Port 8080 (Configurable via --port & --host)        │
│  - Embedded Static Web Assets                          │
│  - WebSocket: /ws/terminal/:host_id                    │
│  - WebSocket: /ws/metrics/:host_id                     │
│  - REST API: /api/hosts, /api/settings, /api/sftp      │
└──────────────────────────┬─────────────────────────────┘
                           │ Native Async Core (Tokio/russh)
                           ▼
┌────────────────────────────────────────────────────────┐
│                      redash-core                       │
│  - SSH Connection Pooling                              │
│  - SFTP Session & Atomic I/O                           │
│  - Linux/Darwin Metrics Probes                         │
│  - AI Agent Detection Pipeline                         │
└────────────────────────────────────────────────────────┘
```

---

## 🚀 Quick Start

### 1. Launch via Helper Script
```bash
./scripts/run_web.sh [PORT] [HOST]
# Default is http://127.0.0.1:8080
```

### 2. Launch via Cargo
```bash
cargo run --bin redash-server -- --host 127.0.0.1 --port 8080
```

### 3. Run in Docker
```bash
docker build -t redash-web .
docker run -d -p 8080:8080 -v ~/.config/redash:/root/.config/redash redash-web
```

---

## 📡 API Endpoints

### Static Web Assets
- `GET /` - Main Web application HTML shell
- `GET /assets/app.js` - Client engine JavaScript
- `GET /assets/style.css` - DarkTech theme stylesheet

### REST API
- `GET /api/hosts` - List all configured hosts
- `POST /api/hosts` - Create or update a host
- `GET /api/hosts/:id` - Get host details
- `DELETE /api/hosts/:id` - Delete a host
- `POST /api/hosts/:id/test` - Test SSH connectivity
- `GET /api/settings` - Retrieve global settings
- `POST /api/settings` / `PUT /api/settings` - Update settings
- `GET /api/sftp/:host_id/list?path=...` - List remote files and directories
- `GET /api/sftp/:host_id/read?path=...` - Read file content (up to 2MB)
- `POST /api/sftp/:host_id/write` - Save remote file (atomic replacement)

### Real-Time WebSocket Channels
- `ws://<host>:<port>/ws/terminal/:host_id?cols=120&rows=40`
  - Bidirectional streaming between web browser and remote PTY.
  - Automatic detection of AI Agents (`Claude Code`, `Aider`, `Antigravity`, `Kimi`, `Codestral`) with real-time HUD reporting status, cost (USD), and token counts.
- `ws://<host>:<port>/ws/metrics/:host_id`
  - Continuous streaming of `NodeMetrics` (CPU, Memory, Disk, Network RX/TX, Uptime, Load).
