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

/* ---------- WiFi 列表 ----------
 * 以 SSID 为键复用行元素：刷新只更新变化的字段，
 * 不整体重建列表，避免周期性刷新造成的闪烁。
 */

function signalLevel(signal) {
  if (signal >= 75) return 4;
  if (signal >= 50) return 3;
  if (signal >= 25) return 2;
  return 1;
}

function wifiRowHtml(net, i) {
  const level = signalLevel(net.signal);
  const bars = Array.from({ length: 4 }, (_, k) => `<i class="${k < level ? "on" : ""}"></i>`).join("");
  let badges = "";
  if (net.saved) {
    badges += `<span class="badge saved">已存密码</span>`;
    if (!net.connected) badges += `<button class="connect-btn" data-ssid="${escapeHtml(net.ssid)}">连接</button>`;
  } else {
    badges += `<span class="badge unsaved">未存密码</span>`;
  }
  if (!net.secured) badges += `<span class="badge open-net">开放</span>`;
  return `
    <span class="idx">${i + 1}.</span>
    <span class="ssid">${escapeHtml(net.ssid)}</span>
    <span class="badges">${badges}</span>
    <span class="spacer"></span>
    <span class="signal-bars">${bars}</span>
    <span class="pct">${net.signal}%</span>`;
}

function renderWifiList(networks) {
  if (networks.length === 0) {
    els.wifiList.replaceChildren();
    const li = document.createElement("li");
    li.className = "wifi-row";
    li.innerHTML = `<span class="ssid" style="font-weight:400;color:var(--ink-faint)">未扫描到网络</span>`;
    els.wifiList.appendChild(li);
    setStatus("offline", "未连接到任何网络");
    return;
  }

  const existing = new Map();
  for (const row of els.wifiList.children) {
    if (row.dataset.ssid) existing.set(row.dataset.ssid, row);
  }

  const rows = [];
  networks.forEach((net, i) => {
    let li = existing.get(net.ssid);
    const rowClass = "wifi-row" + (net.connected ? " connected" : "");
    if (li) {
      // 复用已有行：仅更新内容，不播放进入动画
      li.className = rowClass;
      li.innerHTML = wifiRowHtml(net, i);
    } else {
      li = document.createElement("li");
      li.dataset.ssid = net.ssid;
      li.className = "wifi-row row-new" + (net.connected ? " connected" : "");
      li.style.animationDelay = `${Math.min(i * 30, 300)}ms`;
      li.innerHTML = wifiRowHtml(net, i);
    }
    rows.push(li);
  });
  // replaceChildren 重排既有节点而不重建，已存在的行不触发闪烁
  els.wifiList.replaceChildren(...rows);

  const connected = networks.find((n) => n.connected);
  if (connected) {
    setStatus("online", `已连接：${connected.ssid}`);
    refreshConnectionInfo();
  } else {
    setStatus("offline", "未连接到任何网络");
  }
}

/* ---------- 行内手动连接 ---------- */

els.wifiList.addEventListener("click", async (event) => {
  const btn = event.target.closest(".connect-btn");
  if (!btn) return;
  const ssid = btn.dataset.ssid;
  btn.disabled = true;
  btn.textContent = "连接中…";
  btn.closest(".wifi-row")?.classList.add("connecting-flash");
  try {
    await invoke("connect_wifi", { ssid });
  } catch (e) {
    setStatus("offline", `连接 ${ssid} 失败：${e}`);
  }
  // 连接结果由周期刷新的列表自然呈现；延时后恢复按钮状态
  setTimeout(() => {
    btn.closest(".wifi-row")?.classList.remove("connecting-flash");
  }, 4000);
});

/* ---------- 开源仓库链接 ---------- */

$("repo-link").addEventListener("click", (event) => {
  event.preventDefault();
  invoke("open_repo");
});

/* ---------- 手动刷新 ---------- */

$("refresh-btn").addEventListener("click", async (event) => {
  const btn = event.currentTarget;
  btn.classList.remove("spin");
  void btn.offsetWidth; // 重新触发动画
  btn.classList.add("spin");
  try {
    const networks = await invoke("get_wifi_list");
    renderWifiList(networks);
  } catch (e) {
    setStatus("offline", `扫描失败：${e}`);
  }
});

/* ---------- 当前连接详情 ---------- */

async function refreshConnectionInfo() {
  try {
    const info = await invoke("get_connection_info");
    if (info) {
      const rate = Math.max(info.receiveRateMbps, info.transmitRateMbps);
      const parts = [
        info.ssid,
        info.band,
        info.channel ? `信道 ${info.channel}` : "",
        rate > 0 ? `${rate} Mbps` : "",
      ].filter(Boolean);
      setStatus("online", `已连接：${parts.join(" · ")}`);
    }
  } catch {
    /* 详情获取失败时保留基础状态文本 */
  }
}
setInterval(refreshConnectionInfo, 6000);

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

