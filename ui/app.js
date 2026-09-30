/* The WiFi Herald — 前端逻辑
 * 通过 window.__TAURI__ 与 Rust 后端交互：
 *   invoke: get_wifi_list / get_snapshot / set_auto_reconnect / autostart_is_enabled / autostart_set
 *   event : wifi-list（网络列表）/ monitor（监控状态快照）
 */

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);

const els = {
  date: $("masthead-date"),
  clock: $("masthead-clock"),
  statusDot: $("status-dot"),
  statusText: $("status-text"),
  wifiList: $("wifi-list"),
  autoSwitch: $("auto-switch"),
  autostartSwitch: $("autostart-switch"),
  views: {
    standby: $("view-standby"),
    online: $("view-online"),
    recovering: $("view-recovering"),
  },
  roundLine: $("round-line"),
  attemptList: $("attempt-list"),
  log: $("log"),
};

/* ---------- 报头时钟 ---------- */

const WEEKDAYS = ["日", "一", "二", "三", "四", "五", "六"];

function tickClock() {
  const now = new Date();
  const pad = (n) => String(n).padStart(2, "0");
  els.date.textContent =
    `${now.getFullYear()} 年 ${now.getMonth() + 1} 月 ${now.getDate()} 日 · 星期${WEEKDAYS[now.getDay()]}`;
  els.clock.textContent = `${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}`;
}
setInterval(tickClock, 1000);
tickClock();

/* ---------- 开关 ---------- */

function setSwitch(el, on) {
  el.classList.toggle("on", on);
  el.setAttribute("aria-checked", String(on));
}

els.autoSwitch.addEventListener("click", async () => {
  const next = !els.autoSwitch.classList.contains("on");
  setSwitch(els.autoSwitch, next);
  const snapshot = await invoke("set_auto_reconnect", { enabled: next });
  renderMonitor(snapshot);
});

els.autostartSwitch.addEventListener("click", async () => {
  const next = !els.autostartSwitch.classList.contains("on");
  const actual = await invoke("autostart_set", { enabled: next });
  setSwitch(els.autostartSwitch, actual);
});

/* ---------- WiFi 列表 ---------- */

function signalLevel(signal) {
  if (signal >= 75) return 4;
  if (signal >= 50) return 3;
  if (signal >= 25) return 2;
  return 1;
}

function renderWifiList(networks) {
  els.wifiList.innerHTML = "";
  if (networks.length === 0) {
    const li = document.createElement("li");
    li.className = "wifi-row";
    li.innerHTML = `<span class="ssid" style="font-weight:400;color:var(--ink-faint)">未扫描到网络</span>`;
    els.wifiList.appendChild(li);
    return;
  }
  networks.forEach((net, i) => {
    const li = document.createElement("li");
    li.className = "wifi-row" + (net.connected ? " connected" : "");
    li.style.animationDelay = `${Math.min(i * 30, 300)}ms`;

    const level = signalLevel(net.signal);
    const bars = Array.from({ length: 4 }, (_, k) => `<i class="${k < level ? "on" : ""}"></i>`).join("");

    let badges = "";
    if (net.saved) badges += `<span class="badge saved">已存密码</span>`;
    else badges += `<span class="badge unsaved">未存密码</span>`;
    if (!net.secured) badges += `<span class="badge open-net">开放</span>`;

    li.innerHTML = `
      <span class="idx">${i + 1}.</span>
      <span class="ssid">${escapeHtml(net.ssid)}</span>
      ${badges}
      <span class="spacer"></span>
      <span class="signal-bars">${bars}</span>
      <span class="pct">${net.signal}%</span>`;
    els.wifiList.appendChild(li);
  });

  const connected = networks.find((n) => n.connected);
  if (connected) {
    setStatus("online", `已连接：${connected.ssid}`);
  } else {
    setStatus("offline", "未连接到任何网络");
  }
}

function setStatus(kind, text) {
  els.statusDot.className = "dot " + kind;
  els.statusText.textContent = text;
}

/* ---------- 监控状态 ---------- */

const PHASE_VIEW = {
  disabled: "standby",
  monitoring: "online",
  recovering: "recovering",
};

const CANDIDATE_RESULT = {
  pending: "等待",
  connecting: "连接中…",
  failed: "失败",
  success: "成功 ✓",
};

function renderMonitor(snapshot) {
  setSwitch(els.autoSwitch, snapshot.enabled);

  const view = snapshot.enabled ? PHASE_VIEW[snapshot.phase] ?? "standby" : "standby";
  for (const [name, el] of Object.entries(els.views)) {
    el.classList.toggle("hidden", name !== view);
  }

  if (view === "recovering") {
    els.roundLine.textContent = `第 ${snapshot.round} 轮尝试 · 共 ${snapshot.candidates.length} 个候选`;
    renderAttempts(snapshot);
  }

  if (view === "online") {
    setStatus("online", els.statusText.textContent);
  } else if (view === "recovering") {
    setStatus("offline", "网络中断，正在重连");
  }

  renderLog(snapshot.log);
}

function renderAttempts(snapshot) {
  els.attemptList.innerHTML = "";
  snapshot.candidates.forEach((c, i) => {
    const li = document.createElement("li");
    const isCurrent = i === snapshot.current;
    const status = isCurrent && c.status === "connecting" ? "connecting" : c.status;
    li.className = `attempt-row ${status}`;
    li.style.animationDelay = `${Math.min(i * 60, 400)}ms`;
    li.innerHTML = `
      <span class="cursor"></span>
      <span class="a-ssid">${escapeHtml(c.ssid)}</span>
      <span class="spacer"></span>
      <span class="a-signal">${c.signal}%</span>
      <span class="result">${CANDIDATE_RESULT[status] ?? status}</span>`;
    els.attemptList.appendChild(li);
  });
}

function renderLog(lines) {
  els.log.innerHTML = "";
  if (!lines || lines.length === 0) {
    const li = document.createElement("li");
    li.className = "log-empty";
    li.textContent = "暂无记录";
    els.log.appendChild(li);
    return;
  }
  for (const line of lines) {
    const li = document.createElement("li");
    li.textContent = line;
    els.log.appendChild(li);
  }
  els.log.scrollTop = els.log.scrollHeight;
}

/* ---------- 工具 ---------- */

function escapeHtml(text) {
  return text.replace(/[&<>"']/g, (ch) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  }[ch]));
}

/* ---------- 初始化 ---------- */

async function init() {
  await listen("wifi-list", (event) => renderWifiList(event.payload));
  await listen("monitor", (event) => renderMonitor(event.payload));

  const [networks, snapshot, autostart] = await Promise.all([
    invoke("get_wifi_list"),
    invoke("get_snapshot"),
    invoke("autostart_is_enabled"),
  ]);
  renderWifiList(networks);
  renderMonitor(snapshot);
  setSwitch(els.autostartSwitch, autostart);
}

init().catch((err) => setStatus("offline", `初始化失败：${err}`));
