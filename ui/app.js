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
  waveSetState(view, snapshot);

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

/* ---------- 连通性示波器 ----------
 * 波形映射连通性，颜色：在线墨色，恢复中红色，待命灰色。
 * 在线：ASK 幅度调制——比特序列编码为载波上的高斯波包串；
 * 恢复中：高斯波包自左向右传播（连接请求），成功锁定为连续载波，失败衰减为噪声；
 * 待命：微幅噪声。
 */

const wave = {
  canvas: $("wave"),
  mode: "idle", // idle | ask | packet | locked | decay
  amp: 0,       // 当前振幅（平滑过渡）
  targetAmp: 0,
  color: "#1a1a1a",
  phase: 0,
  t: 0,
  packetX: -60, // 波包中心横坐标
  bits: [1, 0, 1, 1, 0, 1, 0, 0, 1, 0], // ASK 编码的比特序列，循环发送
};

function waveSetState(view, snapshot) {
  if (view === "online") {
    wave.mode = "ask";
    wave.targetAmp = 1;
    wave.color = "#1a1a1a";
  } else if (view === "recovering") {
    wave.color = "#8f1d1d";
    if (snapshot.current >= 0) {
      wave.mode = "packet";
      wave.targetAmp = 0.55;
    } else if (snapshot.candidates.some((c) => c.status === "success")) {
      wave.mode = "locked";
      wave.targetAmp = 1;
      wave.color = "#1a1a1a";
    } else {
      wave.mode = "decay";
      wave.targetAmp = 0.08;
    }
  } else {
    wave.mode = "idle";
    wave.targetAmp = 0.15;
    wave.color = "#8a8378";
  }
}

function waveResize() {
  const c = wave.canvas;
  const dpr = window.devicePixelRatio || 1;
  const rect = c.getBoundingClientRect();
  c.width = Math.max(1, Math.round(rect.width * dpr));
  c.height = Math.max(1, Math.round(rect.height * dpr));
}

/* ASK 包络：比特序列编码为比特位中心的高斯凸起，比特 0 处静默 */
function askEnvelope(x, w) {
  const bitW = 44 * (window.devicePixelRatio || 1);
  const scroll = (wave.t * 1.6) % bitW;
  const idx = Math.floor((x + scroll) / bitW) % wave.bits.length;
  const bit = wave.bits[(idx + wave.bits.length) % wave.bits.length];
  if (!bit) return 0.06;
  const center = Math.floor((x + scroll) / bitW) * bitW - scroll + bitW / 2;
  const d = (x - center) / (bitW * 0.32);
  return 0.25 + 0.75 * Math.exp(-d * d);
}

/* 高斯调制波包：中心 px，宽度随传播扩散（色散） */
function packetEnvelope(x, px) {
  const sigma = 26 + px * 0.02;
  const d = (x - px) / sigma;
  return Math.exp(-d * d);
}

function waveFrame() {
  const c = wave.canvas;
  const ctx = c.getContext("2d");
  const w = c.width;
  const h = c.height;
  const mid = h / 2;
  const dpr = window.devicePixelRatio || 1;
  wave.amp += (wave.targetAmp - wave.amp) * 0.04;
  wave.phase += 0.09;
  wave.t += 1;

  if (wave.mode === "packet") {
    wave.packetX += w * 0.011;
    if (wave.packetX > w + 60 * dpr) wave.packetX = -60 * dpr;
  } else if (wave.mode === "decay") {
    wave.packetX += (w * 0.45 - wave.packetX) * 0.02; // 波包停在中途并衰减
  }

  ctx.clearRect(0, 0, w, h);
  // 中线
  ctx.strokeStyle = "rgba(138,131,120,0.35)";
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(0, mid);
  ctx.lineTo(w, mid);
  ctx.stroke();

  const maxAmp = mid - 6;
  const noiseLevel = (1 - wave.amp) * 7;

  // 波形
  ctx.strokeStyle = wave.color;
  ctx.lineWidth = Math.max(1.5, dpr * 1.2);
  ctx.beginPath();
  for (let x = 0; x <= w; x += 2) {
    const carrier = Math.sin(x * 0.045 - wave.phase);
    let envelope;
    if (wave.mode === "ask") envelope = askEnvelope(x, w);
    else if (wave.mode === "packet" || wave.mode === "decay") envelope = packetEnvelope(x, wave.packetX);
    else envelope = 1;
    const noise = (Math.random() - 0.5) * noiseLevel;
    const y = mid + carrier * envelope * wave.amp * maxAmp + noise;
    if (x === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  }
  ctx.stroke();

  // ASK 模式：底部比特刻度，展示编码分布
  if (wave.mode === "ask") {
    const bitW = 44 * dpr;
    const scroll = (wave.t * 1.6) % bitW;
    ctx.strokeStyle = "rgba(26,26,26,0.5)";
    ctx.lineWidth = 1;
    for (let i = -1; i * bitW < w + bitW; i++) {
      const bx = i * bitW - scroll + bitW / 2;
      const bit = wave.bits[((i % wave.bits.length) + wave.bits.length) % wave.bits.length];
      const tickH = bit ? 8 * dpr : 3 * dpr;
      ctx.beginPath();
      ctx.moveTo(bx, h - 2);
      ctx.lineTo(bx, h - 2 - tickH);
      ctx.stroke();
    }
  }
  requestAnimationFrame(waveFrame);
}

/* ---------- 高斯三维分布曲面（待命） ----------
 * z = exp(-(x²+y²)/(2σ²)) 的线框曲面，等角投影，
 * σ 呼吸起伏，曲面绕竖直轴缓慢旋转。
 */

const gauss = { canvas: $("gauss3d"), t: 0 };

function gaussFrame() {
  const c = gauss.canvas;
  if (!c) return;
  // 仅待命视图可见时绘制
  if (els.views.standby.classList.contains("hidden")) {
    requestAnimationFrame(gaussFrame);
    return;
  }
  const dpr = window.devicePixelRatio || 1;
  const rect = c.getBoundingClientRect();
  c.width = Math.max(1, Math.round(rect.width * dpr));
  c.height = Math.max(1, Math.round(rect.height * dpr));
  const ctx = c.getContext("2d");
  const w = c.width;
  const h = c.height;
  gauss.t += 0.008;

  const sigma = 0.9 + 0.3 * Math.sin(gauss.t * 1.4); // 呼吸
  const rot = gauss.t * 0.35;                         // 绕竖直轴缓转
  const n = 22;                                       // 网格密度
  const range = 2.4;
  const scale = w / (range * 3.4);
  const zScale = h * 0.52;
  const cx = w / 2;
  const cy = h * 0.62;

  const project = (x, y, z) => {
    const rx = x * Math.cos(rot) - y * Math.sin(rot);
    const ry = x * Math.sin(rot) + y * Math.cos(rot);
    return [cx + (rx - ry) * 0.866 * scale, cy + (rx + ry) * 0.30 * scale - z * zScale];
  };
  const z = (x, y) => Math.exp(-(x * x + y * y) / (2 * sigma * sigma));

  ctx.clearRect(0, 0, w, h);
  ctx.strokeStyle = "rgba(138,131,120,0.75)";
  ctx.lineWidth = 1;
  // 沿 x 方向的网格线
  for (let j = 0; j <= n; j++) {
    const y = -range + (2 * range * j) / n;
    ctx.beginPath();
    for (let i = 0; i <= n; i++) {
      const x = -range + (2 * range * i) / n;
      const [sx, sy] = project(x, y, z(x, y));
      if (i === 0) ctx.moveTo(sx, sy);
      else ctx.lineTo(sx, sy);
    }
    ctx.stroke();
  }
  // 沿 y 方向的网格线
  for (let i = 0; i <= n; i++) {
    const x = -range + (2 * range * i) / n;
    ctx.beginPath();
    for (let j = 0; j <= n; j++) {
      const y = -range + (2 * range * j) / n;
      const [sx, sy] = project(x, y, z(x, y));
      if (j === 0) ctx.moveTo(sx, sy);
      else ctx.lineTo(sx, sy);
    }
    ctx.stroke();
  }
  requestAnimationFrame(gaussFrame);
}

window.addEventListener("resize", waveResize);

/* ---------- 工具 ---------- */

function escapeHtml(text) {
  return text.replace(/[&<>"']/g, (ch) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  }[ch]));
}

/* ---------- 初始化 ---------- */

async function init() {
  waveResize();
  requestAnimationFrame(waveFrame);
  requestAnimationFrame(gaussFrame);

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
