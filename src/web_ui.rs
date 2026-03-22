/// Embedded single-page web UI for catdns.
/// Served as a static HTML string — no external dependencies.
pub const INDEX_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>catdns dashboard</title>
<style>
:root {
  --bg: #0f1117;
  --surface: #1a1d27;
  --surface2: #242838;
  --border: #2e3348;
  --text: #e1e4ed;
  --text2: #8b90a5;
  --accent: #6c8cff;
  --accent2: #4ecdc4;
  --green: #4ecdc4;
  --red: #ff6b6b;
  --orange: #ffa94d;
  --yellow: #ffd43b;
  --font: 'SF Mono', 'Cascadia Code', 'Fira Code', 'Consolas', monospace;
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body { background: var(--bg); color: var(--text); font-family: var(--font); font-size: 13px; min-height: 100vh; }
a { color: var(--accent); text-decoration: none; }

/* Header */
.header {
  background: var(--surface);
  border-bottom: 1px solid var(--border);
  padding: 12px 24px;
  display: flex; align-items: center; gap: 20px;
}
.header h1 { font-size: 16px; font-weight: 600; color: var(--accent); }
.header .pill {
  background: var(--surface2); border: 1px solid var(--border);
  border-radius: 12px; padding: 2px 10px; font-size: 11px; color: var(--text2);
}

/* Tabs */
.tabs {
  display: flex; gap: 0; background: var(--surface);
  border-bottom: 1px solid var(--border); padding: 0 24px;
}
.tab {
  padding: 10px 20px; cursor: pointer; font-size: 12px;
  color: var(--text2); border-bottom: 2px solid transparent;
  transition: all 0.15s;
}
.tab:hover { color: var(--text); }
.tab.active { color: var(--accent); border-bottom-color: var(--accent); }

/* Content */
.content { padding: 20px 24px; max-width: 1400px; margin: 0 auto; }
.panel { display: none; }
.panel.active { display: block; }

/* Cards */
.cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 12px; margin-bottom: 20px; }
.card {
  background: var(--surface); border: 1px solid var(--border);
  border-radius: 8px; padding: 16px;
}
.card .label { font-size: 11px; color: var(--text2); text-transform: uppercase; letter-spacing: 0.5px; margin-bottom: 6px; }
.card .value { font-size: 24px; font-weight: 700; }
.card .sub { font-size: 11px; color: var(--text2); margin-top: 4px; }

/* Tables */
.table-wrap {
  background: var(--surface); border: 1px solid var(--border);
  border-radius: 8px; overflow: hidden;
}
.table-wrap h3 {
  padding: 12px 16px; font-size: 13px; font-weight: 600;
  border-bottom: 1px solid var(--border); color: var(--text2);
}
table { width: 100%; border-collapse: collapse; }
th {
  text-align: left; padding: 8px 16px; font-size: 11px;
  color: var(--text2); text-transform: uppercase; letter-spacing: 0.5px;
  border-bottom: 1px solid var(--border); background: var(--surface2);
}
td { padding: 7px 16px; border-bottom: 1px solid var(--border); font-size: 12px; }
tr:last-child td { border-bottom: none; }
tr:hover td { background: var(--surface2); }

/* Badges */
.badge {
  display: inline-block; padding: 2px 8px; border-radius: 10px;
  font-size: 10px; font-weight: 600;
}
.badge-green { background: rgba(78,205,196,0.15); color: var(--green); }
.badge-red { background: rgba(255,107,107,0.15); color: var(--red); }
.badge-orange { background: rgba(255,169,77,0.15); color: var(--orange); }
.badge-blue { background: rgba(108,140,255,0.15); color: var(--accent); }

/* Progress bar */
.progress { background: var(--surface2); border-radius: 4px; height: 8px; overflow: hidden; }
.progress-fill { height: 100%; border-radius: 4px; transition: width 0.3s; }

/* Config editor */
.editor-wrap {
  background: var(--surface); border: 1px solid var(--border);
  border-radius: 8px; overflow: hidden;
}
.editor-toolbar {
  display: flex; align-items: center; justify-content: space-between;
  padding: 10px 16px; border-bottom: 1px solid var(--border); background: var(--surface2);
}
.editor-toolbar h3 { font-size: 13px; font-weight: 600; color: var(--text2); }
.editor-actions { display: flex; gap: 8px; }
textarea#config-editor {
  width: 100%; min-height: 500px; padding: 16px;
  background: var(--surface); color: var(--text); border: none;
  font-family: var(--font); font-size: 13px; line-height: 1.6;
  resize: vertical; outline: none;
}
.btn {
  padding: 6px 16px; border-radius: 6px; border: 1px solid var(--border);
  font-family: var(--font); font-size: 12px; cursor: pointer;
  transition: all 0.15s;
}
.btn-primary { background: var(--accent); color: #fff; border-color: var(--accent); }
.btn-primary:hover { opacity: 0.85; }
.btn-secondary { background: var(--surface2); color: var(--text); }
.btn-secondary:hover { background: var(--border); }
.btn-danger { background: var(--red); color: #fff; border-color: var(--red); }
.btn-danger:hover { opacity: 0.85; }

/* Toast */
.toast {
  position: fixed; bottom: 24px; right: 24px;
  padding: 10px 20px; border-radius: 8px;
  font-size: 12px; font-weight: 600; z-index: 1000;
  transform: translateY(80px); opacity: 0;
  transition: all 0.3s;
}
.toast.show { transform: translateY(0); opacity: 1; }
.toast-ok { background: var(--green); color: #000; }
.toast-err { background: var(--red); color: #fff; }

/* Refresh indicator */
.refresh-dot {
  width: 6px; height: 6px; border-radius: 50%;
  background: var(--green); display: inline-block;
  animation: pulse 2s infinite;
}
@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.3; } }

/* Chart area */
.chart-bar { display: flex; align-items: end; gap: 2px; height: 60px; margin-top: 8px; }
.chart-bar .bar {
  flex: 1; background: var(--accent); border-radius: 2px 2px 0 0;
  min-height: 1px; transition: height 0.3s;
}

/* Scrollbar */
::-webkit-scrollbar { width: 6px; }
::-webkit-scrollbar-track { background: var(--bg); }
::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }
</style>
</head>
<body>

<div class="header">
  <h1>&#128049; catdns</h1>
  <span class="pill" id="uptime">--</span>
  <span class="pill"><span class="refresh-dot"></span>&nbsp;live</span>
</div>

<div class="tabs">
  <div class="tab active" onclick="switchTab('dashboard')">Dashboard</div>
  <div class="tab" onclick="switchTab('history')">History</div>
  <div class="tab" onclick="switchTab('upstreams')">Upstreams</div>
  <div class="tab" onclick="switchTab('config')">Config</div>
</div>

<div class="content">

<!-- Dashboard -->
<div class="panel active" id="panel-dashboard">
  <div class="cards" id="stat-cards"></div>
  <div style="display:grid; grid-template-columns: 1fr 1fr; gap: 12px;">
    <div class="table-wrap">
      <h3>Cache</h3>
      <table><tbody id="cache-table"></tbody></table>
    </div>
    <div class="table-wrap">
      <h3>Query Rate (last 30 polls)</h3>
      <div style="padding: 16px;">
        <div class="chart-bar" id="qps-chart"></div>
        <div style="display:flex; justify-content:space-between; margin-top:4px;">
          <span style="font-size:10px; color:var(--text2);">older</span>
          <span style="font-size:10px; color:var(--text2);" id="qps-label">-- qps</span>
          <span style="font-size:10px; color:var(--text2);">now</span>
        </div>
      </div>
    </div>
  </div>
</div>

<!-- History -->
<div class="panel" id="panel-history">
  <div style="display:flex; gap:8px; margin-bottom:12px; align-items:center;">
    <input id="hist-filter" type="text" placeholder="filter domain..."
      style="flex:1; padding:8px 12px; background:var(--surface); border:1px solid var(--border); border-radius:6px; color:var(--text); font-family:var(--font); font-size:12px; outline:none;">
    <span style="font-size:11px; color:var(--text2);" id="hist-count">0 queries</span>
  </div>
  <div class="table-wrap">
    <table>
      <thead><tr><th>Time</th><th>Domain</th><th>Type</th><th>Route</th><th>Cache</th><th>Latency</th></tr></thead>
      <tbody id="history-table"></tbody>
    </table>
  </div>
</div>

<!-- Upstreams -->
<div class="panel" id="panel-upstreams">
  <div style="display:grid; grid-template-columns: 1fr 1fr; gap: 12px;">
    <div class="table-wrap">
      <h3>China Upstreams</h3>
      <table>
        <thead><tr><th>Address</th><th>Queries</th><th>Failures</th><th>Weight</th><th>Health</th></tr></thead>
        <tbody id="china-upstream-table"></tbody>
      </table>
    </div>
    <div class="table-wrap">
      <h3>Global Upstreams</h3>
      <table>
        <thead><tr><th>Address</th><th>Queries</th><th>Failures</th><th>Weight</th><th>Health</th></tr></thead>
        <tbody id="global-upstream-table"></tbody>
      </table>
    </div>
  </div>
</div>

<!-- Config -->
<div class="panel" id="panel-config">
  <div class="editor-wrap">
    <div class="editor-toolbar">
      <h3>config.json</h3>
      <div class="editor-actions">
        <button class="btn btn-secondary" onclick="loadConfig()">Reload</button>
        <button class="btn btn-primary" onclick="saveConfig()">Save &amp; Write</button>
      </div>
    </div>
    <textarea id="config-editor" spellcheck="false"></textarea>
  </div>
  <div style="margin-top:8px; font-size:11px; color:var(--text2);">
    Changes are written to disk. A restart is required for most settings to take effect.
  </div>
  <div id="config-error" style="margin-top:8px; font-size:12px; color:var(--red); display:none;"></div>
</div>

</div><!-- /content -->

<div class="toast" id="toast"></div>

<script>
const API = '';
let qpsHistory = [];
let lastTotal = null;
let lastPollTime = null;

function switchTab(name) {
  document.querySelectorAll('.tab').forEach((t, i) => {
    const panels = ['dashboard','history','upstreams','config'];
    t.classList.toggle('active', panels[i] === name);
  });
  document.querySelectorAll('.panel').forEach(p => p.classList.remove('active'));
  document.getElementById('panel-' + name).classList.add('active');
  if (name === 'config') loadConfig();
}

function toast(msg, ok) {
  const t = document.getElementById('toast');
  t.textContent = msg;
  t.className = 'toast show ' + (ok ? 'toast-ok' : 'toast-err');
  setTimeout(() => t.className = 'toast', 2500);
}

function fmtNum(n) {
  if (n >= 1e6) return (n/1e6).toFixed(1) + 'M';
  if (n >= 1e3) return (n/1e3).toFixed(1) + 'K';
  return String(n);
}

function fmtBytes(b) {
  if (b >= 1073741824) return (b/1073741824).toFixed(1) + ' GB';
  if (b >= 1048576) return (b/1048576).toFixed(1) + ' MB';
  if (b >= 1024) return (b/1024).toFixed(1) + ' KB';
  return b + ' B';
}

function fmtUptime(s) {
  const d = Math.floor(s/86400), h = Math.floor(s%86400/3600), m = Math.floor(s%3600/60);
  if (d > 0) return d + 'd ' + h + 'h ' + m + 'm';
  if (h > 0) return h + 'h ' + m + 'm';
  return m + 'm ' + (s%60) + 's';
}

async function fetchStats() {
  try {
    const r = await fetch(API + '/stats');
    const d = await r.json();
    document.getElementById('uptime').textContent = 'uptime: ' + fmtUptime(d.uptime_seconds);

    const now = Date.now();
    if (lastTotal !== null && lastPollTime !== null) {
      const dt = (now - lastPollTime) / 1000;
      const dq = d.handler.total_queries - lastTotal;
      const qps = dt > 0 ? dq / dt : 0;
      qpsHistory.push(qps);
      if (qpsHistory.length > 30) qpsHistory.shift();
    }
    lastTotal = d.handler.total_queries;
    lastPollTime = now;

    // Stat cards
    const h = d.handler, c = d.cache;
    document.getElementById('stat-cards').innerHTML = [
      card('Total Queries', fmtNum(h.total_queries), ''),
      card('China Queries', fmtNum(h.china_queries), pct(h.china_queries, h.total_queries) + ' of total'),
      card('Global Queries', fmtNum(h.global_queries), pct(h.global_queries, h.total_queries) + ' of total'),
      card('Cache Hit Rate', (c.hit_rate * 100).toFixed(1) + '%', fmtNum(c.hits) + ' hits / ' + fmtNum(c.hits + c.misses) + ' lookups'),
      card('Cache Entries', fmtNum(c.entries), fmtBytes(c.bytes_used) + ' used'),
      card('Cache Evictions', fmtNum(c.evictions), ''),
    ].join('');

    // Cache table
    document.getElementById('cache-table').innerHTML = [
      trow('Hits', fmtNum(c.hits)),
      trow('Misses', fmtNum(c.misses)),
      trow('Hit Rate', (c.hit_rate*100).toFixed(2) + '%'),
      trow('Entries', fmtNum(c.entries)),
      trow('Memory Used', fmtBytes(c.bytes_used)),
      trow('Inserts', fmtNum(c.inserts)),
      trow('Evictions', fmtNum(c.evictions)),
    ].join('');

    // QPS chart
    renderQps();
  } catch(e) { console.error('stats fetch error', e); }
}

function card(label, value, sub) {
  return '<div class="card"><div class="label">' + label + '</div><div class="value">' + value + '</div><div class="sub">' + sub + '</div></div>';
}
function trow(k, v) { return '<tr><td style="color:var(--text2)">' + k + '</td><td style="text-align:right;font-weight:600">' + v + '</td></tr>'; }
function pct(a, b) { return b > 0 ? (a/b*100).toFixed(1) + '%' : '0%'; }

function renderQps() {
  const el = document.getElementById('qps-chart');
  if (!qpsHistory.length) { el.innerHTML = '<span style="color:var(--text2);font-size:11px">collecting data...</span>'; return; }
  const max = Math.max(...qpsHistory, 1);
  el.innerHTML = qpsHistory.map(v => {
    const h = Math.max(1, (v/max)*58);
    return '<div class="bar" style="height:' + h + 'px" title="' + v.toFixed(1) + ' qps"></div>';
  }).join('');
  document.getElementById('qps-label').textContent = qpsHistory[qpsHistory.length-1].toFixed(1) + ' qps';
}

async function fetchHistory() {
  try {
    const r = await fetch(API + '/history');
    const data = await r.json();
    const filter = document.getElementById('hist-filter').value.toLowerCase();
    const filtered = filter ? data.filter(d => d.qname.toLowerCase().includes(filter)) : data;
    document.getElementById('hist-count').textContent = filtered.length + ' queries';
    document.getElementById('history-table').innerHTML = filtered.map(q => {
      const t = new Date(q.timestamp);
      const ts = t.toLocaleTimeString();
      const route = q.china
        ? '<span class="badge badge-orange">china</span>'
        : '<span class="badge badge-blue">global</span>';
      const cache = q.cached
        ? '<span class="badge badge-green">hit</span>'
        : '<span class="badge badge-red">miss</span>';
      const lat = q.elapsed_ms < 1 ? '<0.1ms' : q.elapsed_ms.toFixed(1) + 'ms';
      const latColor = q.elapsed_ms < 5 ? 'var(--green)' : q.elapsed_ms < 50 ? 'var(--yellow)' : 'var(--red)';
      return '<tr><td style="color:var(--text2)">' + ts + '</td><td>' + esc(q.qname) + '</td><td>' + q.qtype + '</td><td>' + route + '</td><td>' + cache + '</td><td style="color:'+latColor+'">' + lat + '</td></tr>';
    }).join('');
  } catch(e) { console.error('history fetch error', e); }
}

async function fetchUpstreams() {
  try {
    const r = await fetch(API + '/upstreams');
    const d = await r.json();
    renderUpstreamTable('china-upstream-table', d.china);
    renderUpstreamTable('global-upstream-table', d.global);
  } catch(e) { console.error('upstreams fetch error', e); }
}

function renderUpstreamTable(id, list) {
  document.getElementById(id).innerHTML = list.map(u => {
    const failRate = u.queries > 0 ? (u.failures / u.queries * 100) : 0;
    const health = failRate < 1 ? '<span class="badge badge-green">healthy</span>'
                 : failRate < 10 ? '<span class="badge badge-orange">degraded</span>'
                 : '<span class="badge badge-red">unhealthy</span>';
    return '<tr><td style="font-size:11px">' + esc(u.addr) + '</td><td>' + fmtNum(u.queries) + '</td><td>' + fmtNum(u.failures) + '</td><td>' + u.weight + '</td><td>' + health + '</td></tr>';
  }).join('');
}

async function loadConfig() {
  try {
    const r = await fetch(API + '/config');
    const d = await r.json();
    document.getElementById('config-editor').value = JSON.stringify(d, null, 2);
    document.getElementById('config-error').style.display = 'none';
  } catch(e) {
    toast('Failed to load config', false);
  }
}

async function saveConfig() {
  const text = document.getElementById('config-editor').value;
  const errEl = document.getElementById('config-error');
  try {
    JSON.parse(text); // validate locally first
  } catch(e) {
    errEl.textContent = 'Invalid JSON: ' + e.message;
    errEl.style.display = 'block';
    toast('Invalid JSON', false);
    return;
  }
  errEl.style.display = 'none';
  try {
    const r = await fetch(API + '/config', {
      method: 'PUT',
      headers: {'Content-Type': 'application/json'},
      body: text,
    });
    if (r.ok) {
      toast('Config saved to disk', true);
    } else {
      const d = await r.json();
      errEl.textContent = d.error || 'Save failed';
      errEl.style.display = 'block';
      toast('Save failed', false);
    }
  } catch(e) {
    toast('Save failed: ' + e.message, false);
  }
}

function esc(s) { const d = document.createElement('div'); d.textContent = s; return d.innerHTML; }

// Filter history on typing
document.getElementById('hist-filter').addEventListener('input', fetchHistory);

// Poll
async function poll() {
  await Promise.all([fetchStats(), fetchHistory(), fetchUpstreams()]);
}
poll();
setInterval(poll, 2000);
</script>
</body>
</html>
"##;
