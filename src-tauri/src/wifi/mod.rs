//! WiFi 平台抽象层。
//!
//! 接口（scan / connect / disconnect / connected_ssid / connection_info /
//! trigger_active_scan / wifi_radio_on / ensure_wifi_on）与平台无关，
//! 每个平台一个实现模块，编译期按目标系统选择：
//!
//! - Windows：WLAN API（wlanapi.dll 结构体）
//! - macOS：CoreWLAN（Apple 官方框架）
//! - Linux：NetworkManager（nmcli，桌面发行版标准网络管理器）
//!
//! 后续接入新平台时新增一个实现模块即可，上层（monitor / lib / UI）不感知平台。

use encoding_rs::GBK;
use serde::Serialize;
use std::time::Duration;

/// 一个扫描到的 WiFi 网络。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiNetwork {
    pub ssid: String,
    /// 信号强度，0-100。
    pub signal: u8,
    /// 是否需要密码（开放网络为 false）。
    pub secured: bool,
    /// 是否存在同名配置，存在则可直接连接。
    pub saved: bool,
    /// 是否为当前已连接的网络。
    pub connected: bool,
}

/// 当前连接的详细信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub ssid: String,
    pub band: String,
    pub channel: u32,
    pub receive_rate_mbps: f32,
    pub transmit_rate_mbps: f32,
    pub authentication: String,
    pub signal: u8,
}

/// SSID 原始字节解码：UTF-8 优先（WiFi 标准），失败回退 GBK。
pub(crate) fn ssid_to_string(raw: &[u8]) -> String {
    match String::from_utf8(raw.to_vec()) {
        Ok(text) => text,
        Err(_) => GBK.decode(raw).0.into_owned(),
    }
}

/// 等待网卡关联到指定 SSID，超时返回 false。
pub fn wait_associated(ssid: &str, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if connected_ssid().as_deref() == Some(ssid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(800));
    }
    false
}

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_utf8_ssid() {
        assert_eq!(ssid_to_string("魔法上网".as_bytes()), "魔法上网");
        assert_eq!(ssid_to_string(b"CMCC-u2c6-5G"), "CMCC-u2c6-5G");
    }

    #[test]
    fn decodes_gbk_ssid_fallback() {
        let (gbk, _, _) = GBK.encode("中文热点");
        assert_eq!(ssid_to_string(&gbk), "中文热点");
    }
}
