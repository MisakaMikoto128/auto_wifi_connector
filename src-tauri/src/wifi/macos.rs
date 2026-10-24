//! macOS 实现：CoreWLAN（Apple 官方 Wi-Fi 框架）。
//!
//! 共享类型见 super（mod.rs）。注意事项：
//! - macOS 14+ 系统 requirement：读取 SSID 需要应用获得「定位服务」授权，
//!   未授权时 ssid/ssidData 返回 nil（所有第三方应用一致的系统限制）
//! - 「已保存配置」来自系统的偏好网络列表，经 `networksetup` 官方工具读取
//! - 连接通过 `associateToNetwork` 完成；系统偏好网络中已存在的凭证
//!   由系统钥匙串提供（password 传 nil）

use super::{ssid_to_string, ConnectionInfo, WifiNetwork};
use objc2_core_wlan::{CWChannelBand, CWInterface, CWSecurity, CWWiFiClient};
use objc2_foundation::{NSSet, NSString};
use std::process::Command;

/// 获取默认 Wi-Fi 接口。
fn interface() -> Option<objc2::rc::Retained<CWInterface>> {
    // SAFETY: CoreWLAN 查询接口，无未定义行为
    unsafe { CWWiFiClient::sharedWiFiClient().interface() }
}

/// RSSI（dBm，约 -100 至 -40）映射为 0-100 信号百分比。
/// 参数为 NSInteger（macOS 目标上即 isize）。
fn rssi_to_percent(rssi: isize) -> u8 {
    (((rssi + 100) * 2).clamp(0, 100)) as u8
}

/// 读取系统偏好网络（已保存）的 SSID 列表。
fn saved_ssids(interface_name: &str) -> Vec<String> {
    let Ok(output) = Command::new("networksetup")
        .args(["-listpreferredwirelessnetworks", interface_name])
        .output()
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .skip(1) // 首行为标题行
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// 扫描可见网络并标注已保存配置与当前连接，按信号降序。
pub fn scan() -> Result<Vec<WifiNetwork>, String> {
    // SAFETY: CoreWLAN 扫描查询，无未定义行为
    unsafe {
        let Some(iface) = interface() else {
            return Err("no wifi interface".to_string());
        };
        let iface_name = iface
            .interfaceName()
            .map(|n| n.to_string())
            .unwrap_or_default();
        let saved_list = saved_ssids(&iface_name);
        let set = iface
            .scanForNetworksWithSSID_error(None)
            .map_err(|e| format!("scan failed: {e}"))?;

        let objects = set.allObjects();
        let mut networks: Vec<WifiNetwork> = Vec::new();
        for i in 0..objects.count() {
            let net = objects.objectAtIndex(i);
            let Some(ssid_data) = net.ssidData() else {
                continue;
            };
            let ssid = ssid_to_string(&ssid_data.to_vec());
            if ssid.is_empty() {
                continue;
            }
            let signal = rssi_to_percent(net.rssiValue());
            let secured = !net.supportsSecurity(CWSecurity::None);
            let saved = saved_list.iter().any(|s| s == &ssid);
            if let Some(existing) = networks.iter_mut().find(|e| e.ssid == ssid) {
                existing.signal = existing.signal.max(signal);
                existing.saved |= saved;
            } else {
                networks.push(WifiNetwork {
                    ssid,
                    signal,
                    secured,
                    saved,
                    connected: false,
                });
            }
        }
        // 当前连接的网络由接口状态标注
        if let Some(ssid_data) = iface.ssidData() {
            let current = ssid_to_string(&ssid_data.to_vec());
            for n in &mut networks {
                n.connected = n.ssid == current;
            }
        }
        networks.sort_by_key(|n| std::cmp::Reverse(n.signal));
        Ok(networks)
    }
}

/// 当前连接的 SSID。
pub fn connected_ssid() -> Option<String> {
    // SAFETY: CoreWLAN 状态查询，无未定义行为
    unsafe {
        let iface = interface()?;
        let data = iface.ssidData()?;
        Some(ssid_to_string(&data.to_vec()))
    }
}

/// 当前连接详情；未连接时返回 None。
pub fn connection_info() -> Option<ConnectionInfo> {
    // SAFETY: CoreWLAN 状态查询，无未定义行为
    unsafe {
        let iface = interface()?;
        let ssid_data = iface.ssidData()?;
        let ssid = ssid_to_string(&ssid_data.to_vec());
        let signal = rssi_to_percent(iface.rssiValue());
        let channel = iface.wlanChannel()?;
        let channel_number = channel.channelNumber() as u32;
        let band = match channel.channelBand() {
            CWChannelBand::Band2GHz => "2.4 GHz",
            CWChannelBand::Band5GHz => "5 GHz",
            CWChannelBand::Band6GHz => "6 GHz",
            _ => "",
        };
        Some(ConnectionInfo {
            ssid,
            band: band.to_string(),
            channel: channel_number,
            receive_rate_mbps: 0.0,
            transmit_rate_mbps: 0.0,
            authentication: String::new(),
            signal,
        })
    }
}

/// 发起连接。在扫描结果中定位目标网络后由系统钥匙串凭证关联。
/// 返回 Ok 仅表示系统接受了关联请求。
pub fn connect(ssid: &str) -> Result<(), String> {
    // SAFETY: CoreWLAN 关联操作，无未定义行为
    unsafe {
        let Some(iface) = interface() else {
            return Err("no wifi interface".to_string());
        };
        let set = iface
            .scanForNetworksWithSSID_error(None)
            .map_err(|e| format!("scan failed: {e}"))?;
        let objects = set.allObjects();
        let target = (0..objects.count())
            .map(|i| objects.objectAtIndex(i))
            .find(|net| {
                net.ssidData()
                    .is_some_and(|d| ssid_to_string(&d.to_vec()) == ssid)
            });
        let Some(network) = target else {
            return Err(format!("network {ssid} not found in scan"));
        };
        iface
            .associateToNetwork_password_error(&network, None::<&NSString>)
            .map_err(|e| format!("associate failed: {e}"))
    }
}

/// 断开当前 WiFi 连接。
pub fn disconnect() -> Result<(), String> {
    // SAFETY: CoreWLAN 断开操作，无未定义行为
    unsafe {
        let Some(iface) = interface() else {
            return Err("no wifi interface".to_string());
        };
        iface.disassociate();
        Ok(())
    }
}

/// 触发网卡主动扫描。CoreWLAN 的扫描本身即为主动扫描，此处执行一次并丢弃结果。
pub fn trigger_active_scan() -> bool {
    // SAFETY: CoreWLAN 扫描，无未定义行为
    unsafe {
        let Some(iface) = interface() else {
            return false;
        };
        iface.scanForNetworksWithSSID_error(None).is_ok()
    }
}

/// WiFi 无线电是否打开。
pub fn wifi_radio_on() -> Option<bool> {
    // SAFETY: CoreWLAN 电源状态查询，无未定义行为
    unsafe { Some(interface()?.powerOn()) }
}

/// 确保 WiFi 无线电打开。
pub fn ensure_wifi_on() -> bool {
    // SAFETY: CoreWLAN 电源设置，无未定义行为
    unsafe {
        let Some(iface) = interface() else {
            return false;
        };
        if iface.powerOn() {
            return true;
        }
        iface.setPower_error(true).is_ok() && iface.powerOn()
    }
}
