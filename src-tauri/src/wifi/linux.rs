//! Linux 实现：NetworkManager（官方命令行工具 nmcli）。
//!
//! nmcli 为桌面 Linux 标准网络管理器接口；`-t`（terse）模式输出
//! 机器可解析格式，值中的冒号转义为 `\:`。所有调用固定 `LC_ALL=C`，
//! 输出与系统显示语言无关。
//!
//! 依赖：系统安装并运行 NetworkManager（桌面发行版默认具备）。

use super::{ssid_to_string, ConnectionInfo, WifiNetwork};
use std::process::Command;

/// 执行 nmcli 并返回标准输出。固定 LC_ALL=C。
fn nmcli(args: &[&str]) -> Result<String, String> {
    let output = Command::new("nmcli")
        .env("LC_ALL", "C")
        .args(args)
        .output()
        .map_err(|e| format!("failed to run nmcli: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("nmcli failed: {}", stderr.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// 拆分 terse 模式的一行：以未转义的冒号分隔，值中 `\:` 与 `\\` 反转义。
fn split_terse(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut escaped = false;
    for ch in line.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == ':' {
            fields.push(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
    }
    fields.push(current);
    fields
}

/// 第一个 Wi-Fi 设备名；无则 None。
fn wifi_device() -> Option<String> {
    let out = nmcli(&["-t", "-f", "DEVICE,TYPE", "device", "status"]).ok()?;
    out.lines()
        .filter_map(|l| split_terse(l).first().cloned())
        .zip(out.lines().filter_map(|l| split_terse(l).get(1).cloned()))
        .find(|(_, t)| t == "802-11-wireless")
        .map(|(d, _)| d)
}

/// 已保存的 Wi-Fi 连接名列表（连接名通常与 SSID 一致）。
fn saved_profiles() -> Vec<String> {
    let Ok(out) = nmcli(&["-t", "-f", "NAME,TYPE", "connection", "show"]) else {
        return Vec::new();
    };
    out.lines()
        .filter_map(|l| {
            let f = split_terse(l);
            (f.get(1)?.as_str() == "802-11-wireless").then(|| f[0].clone())
        })
        .collect()
}

/// 扫描可见网络并标注已保存配置与当前连接，按信号降序。
pub fn scan() -> Result<Vec<WifiNetwork>, String> {
    let out = nmcli(&[
        "-t",
        "-f",
        "IN-USE,SSID,SIGNAL,SECURITY",
        "device",
        "wifi",
        "list",
    ])?;
    let saved_list = saved_profiles();
    let mut networks: Vec<WifiNetwork> = Vec::new();
    for line in out.lines() {
        let f = split_terse(line);
        if f.len() < 4 {
            continue;
        }
        let connected = f[0] == "*";
        // SSID 原始值经终端以字节呈现；terse 模式下非法 UTF-8 已转义，直接使用
        let ssid = ssid_to_string(f[1].as_bytes());
        if ssid.is_empty() {
            continue;
        }
        let signal: u8 = f[2].parse().unwrap_or(0);
        let secured = f[3] != "--" && !f[3].is_empty();
        if let Some(existing) = networks.iter_mut().find(|e| e.ssid == ssid) {
            existing.signal = existing.signal.max(signal);
            existing.saved |= saved_list.iter().any(|s| s == &ssid);
            existing.connected |= connected;
        } else {
            networks.push(WifiNetwork {
                ssid,
                signal,
                secured,
                saved: saved_list.iter().any(|s| s == &ssid),
                connected,
            });
        }
    }
    networks.sort_by_key(|n| std::cmp::Reverse(n.signal));
    Ok(networks)
}

/// 当前连接的 SSID。
pub fn connected_ssid() -> Option<String> {
    let out = nmcli(&[
        "-t",
        "--rescan",
        "no",
        "-f",
        "IN-USE,SSID",
        "device",
        "wifi",
        "list",
    ])
    .ok()?;
    out.lines().find_map(|l| {
        let f = split_terse(l);
        (f.first()?.as_str() == "*" && f.len() >= 2 && !f[1].is_empty()).then(|| f[1].clone())
    })
}

/// 当前连接详情；未连接时返回 None。
pub fn connection_info() -> Option<ConnectionInfo> {
    let out = nmcli(&[
        "-t",
        "--rescan",
        "no",
        "-f",
        "IN-USE,SSID,CHAN,FREQ,SIGNAL,SECURITY",
        "device",
        "wifi",
        "list",
    ])
    .ok()?;
    for line in out.lines() {
        let f = split_terse(line);
        if f.first().map(String::as_str) != Some("*") || f.len() < 6 {
            continue;
        }
        let mhz: u32 = f[4].parse().unwrap_or(0);
        let (band, _) = freq_to_band_channel(mhz * 1000);
        return Some(ConnectionInfo {
            ssid: f[1].clone(),
            band,
            channel: f[3].parse().unwrap_or(0),
            receive_rate_mbps: 0.0,
            transmit_rate_mbps: 0.0,
            authentication: if f[5] == "--" {
                "Open".to_string()
            } else {
                f[5].clone()
            },
            signal: f[4].parse().unwrap_or(0),
        });
    }
    None
}

/// 由频率（kHz）换算频段与信道号（nmcli FREQ 单位为 MHz）。
fn freq_to_band_channel(freq_khz: u32) -> (String, u32) {
    let mhz = freq_khz / 1000;
    if mhz >= 5000 {
        ("5 GHz".to_string(), (mhz - 5000) / 5)
    } else if mhz >= 2400 {
        ("2.4 GHz".to_string(), (mhz - 2407) / 5)
    } else {
        (String::new(), 0)
    }
}

/// 发起连接（按 SSID 同名配置连接）。无已保存配置时报错。
pub fn connect(ssid: &str) -> Result<(), String> {
    if !saved_profiles().iter().any(|s| s == ssid) {
        return Err(format!("no saved connection profile for {ssid}"));
    }
    nmcli(&["connection", "up", "id", ssid]).map(|_| ())
}

/// 断开当前 WiFi 连接。
pub fn disconnect() -> Result<(), String> {
    let Some(device) = wifi_device() else {
        return Err("no wifi device".to_string());
    };
    nmcli(&["device", "disconnect", &device]).map(|_| ())
}

/// 触发网卡主动扫描。
pub fn trigger_active_scan() -> bool {
    nmcli(&["device", "wifi", "rescan"]).is_ok()
}

/// WiFi 无线电是否打开。
pub fn wifi_radio_on() -> Option<bool> {
    let out = nmcli(&["radio", "wifi"]).ok()?;
    Some(out.trim() == "enabled")
}

/// 确保 WiFi 无线电打开。
pub fn ensure_wifi_on() -> bool {
    if wifi_radio_on() == Some(true) {
        return true;
    }
    nmcli(&["radio", "wifi", "on"]).is_ok() && wifi_radio_on() == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_terse_lines() {
        // 常规行（IN-USE 为空）
        assert_eq!(
            split_terse(":MyWiFi:75:WPA2"),
            vec!["", "MyWiFi", "75", "WPA2"]
        );
        // 活动行与 SSID 含转义冒号
        assert_eq!(split_terse("*:a\\:b:60:--"), vec!["*", "a:b", "60", "--"]);
        // 反斜杠本身转义
        assert_eq!(split_terse("x\\\\y"), vec!["x\\y"]);
    }

    #[test]
    fn freq_mapping() {
        assert_eq!(freq_to_band_channel(5745000), ("5 GHz".to_string(), 149));
        assert_eq!(freq_to_band_channel(2437000), ("2.4 GHz".to_string(), 6));
    }
}
