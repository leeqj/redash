// ReDash Web Client Engine
let currentView = 'fleet';
let hostsList = [];
let activeTerminalWs = null;
let activeMetricsWs = null;
let activeTerminalHostId = null;

// Initialize Web App
document.addEventListener('DOMContentLoaded', () => {
  initNavigation();
  loadHosts();
  loadSettings();
  initTerminalKeybindings();
});

// View Navigation
function initNavigation() {
  const navItems = document.querySelectorAll('.nav-item[data-view]');
  navItems.forEach(item => {
    item.addEventListener('click', () => {
      const targetView = item.getAttribute('data-view');
      switchView(targetView);
    });
  });
}

function switchView(viewName) {
  currentView = viewName;
  document.querySelectorAll('.nav-item').forEach(el => el.classList.remove('active'));
  document.querySelectorAll('.view-container').forEach(el => el.classList.remove('active'));

  const activeNav = document.querySelector(`.nav-item[data-view="${viewName}"]`);
  if (activeNav) activeNav.classList.add('active');

  const activeViewEl = document.getElementById(`view-${viewName}`);
  if (activeViewEl) activeViewEl.classList.add('active');

  const badge = document.getElementById('current-view-badge');
  if (badge) {
    const titles = {
      fleet: 'Fleet Pulse Matrix',
      terminal: 'Workbench Terminal',
      sftp: 'SFTP File Browser',
      settings: 'Settings & Theme'
    };
    badge.innerText = titles[viewName] || viewName;
  }
}

// Host Management
async function loadHosts() {
  try {
    const res = await fetch('/api/hosts');
    const json = await res.json();
    if (json.success && json.data) {
      hostsList = json.data;
      renderFleetGrid(hostsList);
      updateHostSelectors(hostsList);
    }
  } catch (err) {
    console.error('Failed to load hosts:', err);
  }
}

function renderFleetGrid(hosts) {
  const container = document.getElementById('fleet-hosts-grid');
  const countEl = document.getElementById('host-count');
  if (!container) return;

  if (countEl) countEl.innerText = `${hosts.length} Nodes configured`;

  if (hosts.length === 0) {
    container.innerHTML = `
      <div style="grid-column: 1 / -1; padding: 48px; text-align: center; color: var(--text-muted); background: var(--bg-card); border-radius: var(--radius-lg); border: 1px dashed var(--border-muted);">
        <p style="font-size: 14px; margin-bottom: 12px;">No remote hosts connected yet.</p>
        <button class="btn-primary" onclick="openAddHostModal()">+ Add Your First Host</button>
      </div>
    `;
    return;
  }

  container.innerHTML = hosts.map(h => `
    <div class="host-card" id="host-card-${h.id}">
      <div class="host-card-header">
        <div class="host-name">
          <span class="host-status-led" id="led-${h.id}"></span>
          <span>${escapeHtml(h.name)}</span>
        </div>
        <span class="view-badge" style="font-family:'JetBrains Mono';">${escapeHtml(h.target_os || 'Linux')}</span>
      </div>
      <div class="host-meta">${escapeHtml(h.user)}@${escapeHtml(h.hostname)}:${h.port || 22}</div>
      <div class="metrics-row">
        <div class="metric-item">
          <span class="metric-label">CPU Load</span>
          <span class="metric-value" id="cpu-${h.id}">--%</span>
        </div>
        <div class="metric-item">
          <span class="metric-label">Memory</span>
          <span class="metric-value" id="mem-${h.id}">--%</span>
        </div>
        <div class="metric-item">
          <span class="metric-label">Ping Latency</span>
          <span class="metric-value" id="ping-${h.id}">--ms</span>
        </div>
      </div>
      <div class="card-actions">
        <button class="btn-secondary" onclick="openHostTerminal('${h.id}')">Terminal</button>
        <button class="btn-secondary" onclick="openHostSftp('${h.id}')">SFTP</button>
        <button class="btn-secondary" onclick="testHost('${h.id}')" title="Test Connection">Test</button>
      </div>
    </div>
  `).join('');

  // Start background telemetry polling for all cards
  startFleetTelemetry();
}

function updateHostSelectors(hosts) {
  const termSelect = document.getElementById('terminal-host-select');
  const sftpSelect = document.getElementById('sftp-host-select');

  const options = '<option value="">Select Target Host...</option>' + 
    hosts.map(h => `<option value="${h.id}">${escapeHtml(h.name)} (${escapeHtml(h.hostname)})</option>`).join('');

  if (termSelect) termSelect.innerHTML = options;
  if (sftpSelect) sftpSelect.innerHTML = options;
}

function startFleetTelemetry() {
  if (hostsList.length === 0) return;
  // Subscribe to metrics of the first host or cycle
  const firstHost = hostsList[0];
  if (firstHost) {
    connectMetricsWs(firstHost.id);
  }
}

function connectMetricsWs(hostId) {
  if (activeMetricsWs) {
    activeMetricsWs.close();
  }

  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const url = `${proto}//${window.location.host}/ws/metrics/${hostId}`;

  activeMetricsWs = new WebSocket(url);
  activeMetricsWs.onmessage = (event) => {
    try {
      const msg = JSON.parse(event.data);
      if (msg.type === 'metrics' && msg.data) {
        updateHostMetricsDisplay(msg.host_id, msg.data);
      }
    } catch (e) {
      console.error('Failed to parse metrics message:', e);
    }
  };
}

function updateHostMetricsDisplay(hostId, data) {
  const cpuEl = document.getElementById(`cpu-${hostId}`);
  const memEl = document.getElementById(`mem-${hostId}`);
  const pingEl = document.getElementById(`ping-${hostId}`);

  if (cpuEl && data.cpu_usage_percent !== undefined) {
    cpuEl.innerText = `${Math.round(data.cpu_usage_percent)}%`;
  }
  if (memEl && data.mem_usage_percent !== undefined) {
    memEl.innerText = `${Math.round(data.mem_usage_percent)}%`;
  }
  if (pingEl && data.rtt_ms !== undefined && data.rtt_ms !== null) {
    pingEl.innerText = `${data.rtt_ms}ms`;
  }
}

// Terminal WebSocket Client
function openHostTerminal(hostId) {
  switchView('terminal');
  const select = document.getElementById('terminal-host-select');
  if (select) select.value = hostId;
  switchTerminalHost(hostId);
}

function switchTerminalHost(hostId) {
  if (!hostId) return;
  activeTerminalHostId = hostId;

  const host = hostsList.find(h => h.id === hostId);
  const title = document.getElementById('terminal-session-title');
  if (title) title.innerText = host ? `${host.user}@${host.hostname}` : 'Connecting...';

  const screen = document.getElementById('terminal-display');
  if (screen) screen.innerText = `Connecting to ${host ? host.name : hostId} via Web SSH...\n`;

  if (activeTerminalWs) {
    activeTerminalWs.close();
  }

  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const url = `${proto}//${window.location.host}/ws/terminal/${hostId}?cols=100&rows=30`;

  activeTerminalWs = new WebSocket(url);

  activeTerminalWs.onopen = () => {
    if (screen) {
      screen.innerText += `SSH Connection Established.\n\n`;
      screen.focus();
    }
  };

  activeTerminalWs.onmessage = (event) => {
    try {
      const msg = JSON.parse(event.data);
      if (msg.type === 'output') {
        appendTerminalOutput(msg.data);
      } else if (msg.type === 'agent') {
        updateAgentHud(msg);
      } else if (msg.type === 'error') {
        appendTerminalOutput(`\n[ReDash Error] ${msg.message}\n`);
      }
    } catch (e) {
      appendTerminalOutput(event.data);
    }
  };

  activeTerminalWs.onclose = () => {
    appendTerminalOutput(`\n[Session Disconnected]\n`);
  };
}

function appendTerminalOutput(text) {
  const screen = document.getElementById('terminal-display');
  if (!screen) return;
  screen.innerText += text;
  screen.scrollTop = screen.scrollHeight;
}

function clearTerminal() {
  const screen = document.getElementById('terminal-display');
  if (screen) screen.innerText = '';
}

function reconnectTerminal() {
  if (activeTerminalHostId) {
    switchTerminalHost(activeTerminalHostId);
  }
}

function updateAgentHud(agent) {
  const banner = document.getElementById('agent-hud-banner');
  const nameEl = document.getElementById('agent-hud-name');
  const stateEl = document.getElementById('agent-hud-state');
  const metricsEl = document.getElementById('agent-hud-metrics');

  if (banner) banner.style.display = 'flex';
  if (nameEl) nameEl.innerText = agent.name || 'AI Agent';
  if (stateEl) stateEl.innerText = agent.state || 'Active';

  const cost = agent.cost_usd ? `$${agent.cost_usd.toFixed(3)}` : '$0.00';
  const tokens = agent.tokens ? `${agent.tokens} tok` : '0 tok';
  if (metricsEl) metricsEl.innerText = `${cost} | ${tokens}`;
}

function initTerminalKeybindings() {
  const screen = document.getElementById('terminal-display');
  if (!screen) return;

  screen.addEventListener('keydown', (e) => {
    if (!activeTerminalWs || activeTerminalWs.readyState !== WebSocket.OPEN) return;

    let sendData = null;

    if (e.key === 'Enter') {
      sendData = '\r';
    } else if (e.key === 'Backspace') {
      sendData = '\x7f';
    } else if (e.key === 'Tab') {
      e.preventDefault();
      sendData = '\t';
    } else if (e.key === 'ArrowUp') {
      sendData = '\x1b[A';
    } else if (e.key === 'ArrowDown') {
      sendData = '\x1b[B';
    } else if (e.key === 'ArrowRight') {
      sendData = '\x1b[C';
    } else if (e.key === 'ArrowLeft') {
      sendData = '\x1b[D';
    } else if (e.ctrlKey && e.key === 'c') {
      sendData = '\x03';
    } else if (e.ctrlKey && e.key === 'd') {
      sendData = '\x04';
    } else if (e.ctrlKey && e.key === 'l') {
      sendData = '\x0c';
    } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      sendData = e.key;
    }

    if (sendData !== null) {
      e.preventDefault();
      const payload = JSON.stringify({ type: 'input', data: sendData });
      activeTerminalWs.send(payload);
    }
  });
}

// SFTP Browser
function openHostSftp(hostId) {
  switchView('sftp');
  const select = document.getElementById('sftp-host-select');
  if (select) select.value = hostId;
  loadSftpFiles(hostId, '.');
}

async function loadSftpFiles(hostId, path) {
  if (!hostId) return;
  const tbody = document.getElementById('sftp-table-body');
  const pathEl = document.getElementById('sftp-current-path');
  if (pathEl) pathEl.innerText = path || '.';
  if (tbody) tbody.innerHTML = `<tr><td colspan="4" style="padding:20px; text-align:center; color:var(--text-secondary);">Loading remote files...</td></tr>`;

  try {
    const res = await fetch(`/api/sftp/${encodeURIComponent(hostId)}/list?path=${encodeURIComponent(path || '.')}`);
    const json = await res.json();
    if (json.success && json.data) {
      renderSftpTable(hostId, json.data.items || []);
    } else {
      if (tbody) tbody.innerHTML = `<tr><td colspan="4" style="padding:20px; text-align:center; color:var(--status-crit);">${escapeHtml(json.message || 'Failed to list files')}</td></tr>`;
    }
  } catch (err) {
    if (tbody) tbody.innerHTML = `<tr><td colspan="4" style="padding:20px; text-align:center; color:var(--status-crit);">SFTP Error: ${escapeHtml(err.message)}</td></tr>`;
  }
}

function renderSftpTable(hostId, items) {
  const tbody = document.getElementById('sftp-table-body');
  if (!tbody) return;

  if (items.length === 0) {
    tbody.innerHTML = `<tr><td colspan="4" style="padding:24px; text-align:center; color:var(--text-muted);">Directory is empty.</td></tr>`;
    return;
  }

  tbody.innerHTML = items.map(f => `
    <tr style="border-bottom:1px solid var(--border-default); hover:background:var(--bg-input);">
      <td style="padding:10px 14px; font-weight:${f.is_dir ? '600' : '400'}; color:${f.is_dir ? 'var(--accent-cyan)' : 'var(--text-primary)'};">
        ${f.is_dir ? '📁 ' : '📄 '} ${escapeHtml(f.name)}
      </td>
      <td style="padding:10px 14px; font-family:'JetBrains Mono';">${f.is_dir ? '--' : formatBytes(f.size)}</td>
      <td style="padding:10px 14px; font-family:'JetBrains Mono'; color:var(--text-secondary);">0o${f.permissions.toString(8)}</td>
      <td style="padding:10px 14px; text-align:right;">
        ${f.is_dir 
          ? `<button class="btn-secondary" style="padding:2px 8px; font-size:11px;" onclick="loadSftpFiles('${hostId}', '${escapeHtml(f.path)}')">Open</button>` 
          : `<button class="btn-secondary" style="padding:2px 8px; font-size:11px;" onclick="viewRemoteFile('${hostId}', '${escapeHtml(f.path)}')">View</button>`
        }
      </td>
    </tr>
  `).join('');
}

function refreshSftp() {
  const select = document.getElementById('sftp-host-select');
  const pathEl = document.getElementById('sftp-current-path');
  if (select && select.value) {
    loadSftpFiles(select.value, pathEl ? pathEl.innerText : '.');
  }
}

async function viewRemoteFile(hostId, path) {
  try {
    const res = await fetch(`/api/sftp/${encodeURIComponent(hostId)}/read?path=${encodeURIComponent(path)}`);
    const json = await res.json();
    if (json.success && json.data) {
      alert(`[File Content Preview: ${path}]\n\n${json.data.slice(0, 500)}${json.data.length > 500 ? '...' : ''}`);
    } else {
      alert(`Failed to read file: ${json.message}`);
    }
  } catch (err) {
    alert(`Read error: ${err.message}`);
  }
}

// Add Host Modal
function openAddHostModal() {
  const modal = document.getElementById('add-host-modal');
  if (modal) modal.classList.add('active');
}

function closeAddHostModal() {
  const modal = document.getElementById('add-host-modal');
  if (modal) modal.classList.remove('active');
}

async function submitAddHost() {
  const name = document.getElementById('input-host-name').value.trim();
  const hostname = document.getElementById('input-host-ip').value.trim();
  const user = document.getElementById('input-host-user').value.trim();
  const port = parseInt(document.getElementById('input-host-port').value, 10) || 22;
  const target_os = document.getElementById('input-host-os').value;

  if (!name || !hostname || !user) {
    alert('Please fill in Name, IP/Hostname, and User.');
    return;
  }

  const newHost = {
    id: 'host_' + Math.random().toString(36).substring(2, 9),
    name,
    hostname,
    user,
    port,
    auth: 'Password',
    target_os,
    tags: [],
    group: 'Default'
  };

  try {
    const res = await fetch('/api/hosts', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(newHost)
    });
    const json = await res.json();
    if (json.success) {
      closeAddHostModal();
      loadHosts();
    } else {
      alert(`Failed to add host: ${json.message}`);
    }
  } catch (err) {
    alert(`Error: ${err.message}`);
  }
}

async function testHost(hostId) {
  try {
    const res = await fetch(`/api/hosts/${encodeURIComponent(hostId)}/test`, { method: 'POST' });
    const json = await res.json();
    if (json.success) {
      alert('SSH Connection Test: SUCCESS!');
    } else {
      alert(`SSH Connection Test Failed:\n${json.message}`);
    }
  } catch (err) {
    alert(`Test error: ${err.message}`);
  }
}

// Settings
async function loadSettings() {
  try {
    const res = await fetch('/api/settings');
    const json = await res.json();
    if (json.success && json.data) {
      const s = json.data;
      if (document.getElementById('settings-theme')) document.getElementById('settings-theme').value = s.theme_name || 'Minimalist Dark Tech';
      if (document.getElementById('settings-lang')) document.getElementById('settings-lang').value = s.language || 'zh-CN';
      if (document.getElementById('settings-probe-interval')) document.getElementById('settings-probe-interval').value = s.probe_interval_secs || 3;
    }
  } catch (err) {
    console.error('Failed to load settings:', err);
  }
}

async function saveSettings() {
  const theme_name = document.getElementById('settings-theme').value;
  const language = document.getElementById('settings-lang').value;
  const probe_interval_secs = parseInt(document.getElementById('settings-probe-interval').value, 10) || 3;

  try {
    const res = await fetch('/api/settings', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ theme_name, language, probe_interval_secs })
    });
    const json = await res.json();
    if (json.success) {
      alert('Settings saved successfully!');
    }
  } catch (err) {
    alert(`Save error: ${err.message}`);
  }
}

// Utilities
function escapeHtml(str) {
  if (!str) return '';
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}

function formatBytes(bytes) {
  if (!bytes || bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}
