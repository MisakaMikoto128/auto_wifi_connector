//! 断网监控与自动重连轮询。
//!
//! 启用后周期性探测互联网连通性。判定断网时进入恢复流程：
//! 取「可见且已保存配置」的网络作为候选，按信号降序依次尝试连接，
//! 每次连接后验证连通性，任一成功即恢复监控；全部失败则等待后重新开始下一轮。
//! 每次状态变化均向前端推送完整快照（事件名 `monitor`），
//! 并周期性推送 WiFi 列表（事件名 `wifi-list`）。

use crate::{netcheck, wifi};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// 连通性检测周期。
const CHECK_INTERVAL: Duration = Duration::from_secs(10);
/// WiFi 列表推送周期。
const WIFI_LIST_INTERVAL: Duration = Duration::from_secs(6);
/// 单次连接后等待关联完成的超时。
const ASSOCIATE_TIMEOUT: Duration = Duration::from_secs(12);
/// 一轮候选全部失败后的等待时长。
const ROUND_DELAY: Duration = Duration::from_secs(15);
/// 无候选网络时的等待时长。
const NO_CANDIDATE_DELAY: Duration = Duration::from_secs(15);
/// 日志保留条数。
const LOG_CAPACITY: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Disabled,
    Monitoring,
    Recovering,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CandidateStatus {
    Pending,
    Connecting,
    Failed,
    Success,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub ssid: String,
    pub signal: u8,
    pub status: CandidateStatus,
}

/// 推送给前端的完整监控状态快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSnapshot {
    pub enabled: bool,
    pub online: bool,
    pub phase: Phase,
    /// 当前恢复轮次，从 1 开始。
    pub round: u32,
    pub candidates: Vec<Candidate>,
    /// 正在尝试的候选下标，无则为 -1。
    pub current: i32,
    pub log: Vec<String>,
}

impl Default for MonitorSnapshot {
    fn default() -> Self {
        Self {
            enabled: false,
            online: true,
            phase: Phase::Disabled,
            round: 0,
            candidates: Vec::new(),
            current: -1,
            log: Vec::new(),
        }
    }
}

pub type Shared = Arc<Mutex<MonitorSnapshot>>;

pub fn new_shared() -> Shared {
    Arc::new(Mutex::new(MonitorSnapshot::default()))
}

/// 从扫描结果中筛选候选网络：已保存配置、非当前已连接，按信号降序。
pub fn pick_candidates(networks: &[wifi::WifiNetwork]) -> Vec<Candidate> {
    let mut list: Vec<&wifi::WifiNetwork> = networks
        .iter()
        .filter(|n| n.saved && !n.ssid.is_empty())
        .collect();
    list.sort_by(|a, b| b.signal.cmp(&a.signal));
    list.iter()
        .map(|n| Candidate {
            ssid: n.ssid.clone(),
            signal: n.signal,
            status: CandidateStatus::Pending,
        })
        .collect()
}

/// 修改快照并推送克隆到前端。
fn update(app: &AppHandle, shared: &Shared, f: impl FnOnce(&mut MonitorSnapshot)) {
    let snapshot = {
        let mut guard = shared.lock().unwrap();
        f(&mut guard);
        guard.clone()
    };
    let _ = app.emit("monitor", snapshot);
}

fn push_log(app: &AppHandle, shared: &Shared, message: String) {
    update(app, shared, |s| {
        s.log.push(message);
        if s.log.len() > LOG_CAPACITY {
            let excess = s.log.len() - LOG_CAPACITY;
            s.log.drain(..excess);
        }
    });
}

fn is_enabled(shared: &Shared) -> bool {
    shared.lock().unwrap().enabled
}

/// 诊断日志：仅在 debug 构建下写入文件，用于定位现场问题。
#[cfg(debug_assertions)]
fn diag(message: String) {
    use std::io::Write;
    let path = std::env::temp_dir().join("awc-monitor-diag.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{:?}] {message}", std::time::SystemTime::now());
    }
}
#[cfg(not(debug_assertions))]
fn diag(_: String) {}

/// 综合在线判定：WiFi 已关联且互联网可达。
/// 仅做 HTTP 探测会被本机代理虚拟网卡（TUN 模式）误导：
/// WiFi 物理断开后虚拟网卡仍可能响应探测，因此必须同时检查关联状态。
/// HTTP 探测使用时间有界版本，避免 DNS 阻塞导致监控停摆。
fn check_online() -> bool {
    wifi::connected_ssid().is_some() && netcheck::is_online_bounded(Duration::from_secs(12))
}

/// 可中断的睡眠：每 300ms 检查一次启用状态，禁用则提前返回 false。
fn sleep_while_enabled(shared: &Shared, duration: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < duration {
        if !is_enabled(shared) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    true
}

/// 恢复流程：依次尝试候选网络直到连通或功能被禁用。返回是否已恢复。
fn run_recovery(app: &AppHandle, shared: &Shared) -> bool {
    loop {
        if !is_enabled(shared) {
            return false;
        }
        let round = {
            let mut guard = shared.lock().unwrap();
            guard.round += 1;
            guard.round
        };

        let networks = wifi::scan().unwrap_or_default();
        let candidates = pick_candidates(&networks);
        if candidates.is_empty() {
            push_log(app, shared, format!("第 {round} 轮：无可连接的已保存网络，稍后重试"));
            update(app, shared, |s| {
                s.round = round;
                s.candidates.clear();
                s.current = -1;
            });
            if !sleep_while_enabled(shared, NO_CANDIDATE_DELAY) {
                return false;
            }
            continue;
        }

        push_log(
            app,
            shared,
            format!("第 {round} 轮：发现 {} 个候选网络，开始依次尝试", candidates.len()),
        );
        update(app, shared, |s| {
            s.round = round;
            s.candidates = candidates;
            s.current = -1;
        });

        let total = shared.lock().unwrap().candidates.len();
        for i in 0..total {
            if !is_enabled(shared) {
                return false;
            }
            let ssid = shared.lock().unwrap().candidates[i].ssid.clone();
            push_log(app, shared, format!("连接 {ssid}"));
            update(app, shared, |s| {
                s.current = i as i32;
                s.candidates[i].status = CandidateStatus::Connecting;
            });

            diag(format!("connect begin: {ssid}"));
            let conn_result = wifi::connect(&ssid);
            diag(format!("connect returned: {conn_result:?}"));
            let associated = wifi::wait_associated(&ssid, ASSOCIATE_TIMEOUT);
            diag(format!("wait_associated returned: {associated}"));
            let online = associated && netcheck::is_online_retry(2, Duration::from_secs(3));
            diag(format!("is_online_retry returned: {online}"));
            let ok = associated && online;

            if ok {
                push_log(app, shared, format!("{ssid} 连接成功，网络已恢复"));
                update(app, shared, |s| {
                    s.online = true;
                    s.phase = Phase::Monitoring;
                    s.candidates[i].status = CandidateStatus::Success;
                    s.current = -1;
                });
                return true;
            }

            push_log(
                app,
                shared,
                if associated {
                    format!("{ssid} 已关联但无法访问互联网")
                } else {
                    format!("{ssid} 连接失败")
                },
            );
            update(app, shared, |s| {
                s.candidates[i].status = CandidateStatus::Failed;
                s.current = -1;
            });
        }

        push_log(app, shared, format!("第 {round} 轮未恢复网络，{ROUND_DELAY:?} 后开始下一轮"));
        if !sleep_while_enabled(shared, ROUND_DELAY) {
            return false;
        }
    }
}

/// 启动监控线程。
pub fn spawn(app: AppHandle, shared: Shared) {
    std::thread::spawn(move || {
        let mut last_check = Instant::now() - CHECK_INTERVAL;
        let mut last_wifi_list = Instant::now() - WIFI_LIST_INTERVAL;
        loop {
            std::thread::sleep(Duration::from_millis(500));

            let enabled = is_enabled(&shared);
            if !enabled {
                let need_reset = {
                    let guard = shared.lock().unwrap();
                    guard.phase != Phase::Disabled
                };
                if need_reset {
                    update(&app, &shared, |s| {
                        s.phase = Phase::Disabled;
                        s.candidates.clear();
                        s.current = -1;
                        s.round = 0;
                    });
                }
                continue;
            }

            if last_wifi_list.elapsed() >= WIFI_LIST_INTERVAL {
                last_wifi_list = Instant::now();
                if let Ok(list) = wifi::scan() {
                    let _ = app.emit("wifi-list", list);
                }
            }

            if last_check.elapsed() < CHECK_INTERVAL {
                continue;
            }
            last_check = Instant::now();

            if check_online() {
                let was_offline = {
                    let guard = shared.lock().unwrap();
                    !guard.online || guard.phase != Phase::Monitoring
                };
                if was_offline {
                    update(&app, &shared, |s| {
                        s.online = true;
                        s.phase = Phase::Monitoring;
                        s.round = 0;
                        s.candidates.clear();
                        s.current = -1;
                    });
                }
                continue;
            }

            push_log(&app, &shared, "检测到断网，开始自动重连".to_string());
            update(&app, &shared, |s| {
                s.online = false;
                s.phase = Phase::Recovering;
                s.round = 0;
            });
            run_recovery(&app, &shared);
            // 恢复流程返回后重新扫描列表并立即重新计时
            last_check = Instant::now();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wifi::WifiNetwork;

    fn network(ssid: &str, signal: u8, saved: bool) -> WifiNetwork {
        WifiNetwork {
            ssid: ssid.to_string(),
            signal,
            secured: true,
            saved,
            connected: false,
        }
    }

    #[test]
    fn candidates_only_saved_and_sorted_by_signal() {
        let networks = vec![
            network("Weak-Saved", 30, true),
            network("Strong-Unsaved", 95, false),
            network("Strong-Saved", 80, true),
            network("Mid-Saved", 55, true),
        ];
        let candidates = pick_candidates(&networks);
        let ssids: Vec<&str> = candidates.iter().map(|c| c.ssid.as_str()).collect();
        assert_eq!(ssids, vec!["Strong-Saved", "Mid-Saved", "Weak-Saved"]);
        assert!(candidates
            .iter()
            .all(|c| c.status == CandidateStatus::Pending));
    }

    #[test]
    fn candidates_empty_when_nothing_saved() {
        let networks = vec![network("Open1", 90, false), network("Open2", 60, false)];
        assert!(pick_candidates(&networks).is_empty());
        assert!(pick_candidates(&[]).is_empty());
    }

    #[test]
    fn default_snapshot_is_disabled_and_online() {
        let s = MonitorSnapshot::default();
        assert!(!s.enabled);
        assert!(s.online);
        assert_eq!(s.phase, Phase::Disabled);
        assert_eq!(s.current, -1);
    }
}
