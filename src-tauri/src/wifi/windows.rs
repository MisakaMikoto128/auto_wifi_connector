//! Windows 实现：WLAN API（wlanapi.dll 结构体）。
//!
//! 全部数据来自结构体而非文本解析，与系统显示语言、区域编码无关。
//! 共享类型见 super（mod.rs）。

use super::{ssid_to_string, ConnectionInfo, WifiNetwork};
use std::os::windows::process::CommandExt;
use std::process::Command;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::NetworkManagement::WiFi::*;

/// Win32 创建进程标志：不创建控制台窗口。
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// WLAN 会话：客户端句柄与第一个无线接口 GUID 的生命周期封装。
struct Wlan {
    handle: HANDLE,
    interface: windows::core::GUID,
}

impl Wlan {
    fn open() -> Option<Self> {
        unsafe {
            let mut version = 0u32;
            let mut handle = HANDLE::default();
            if WlanOpenHandle(2, None, &mut version, &mut handle) != 0 {
                return None;
            }
            let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
            let mut interface = None;
            if WlanEnumInterfaces(handle, None, &mut list) == 0 && !list.is_null() {
                if (*list).dwNumberOfItems > 0 {
                    interface = Some((*list).InterfaceInfo[0].InterfaceGuid);
                }
                WlanFreeMemory(list as *const core::ffi::c_void);
            }
            match interface {
                Some(guid) => Some(Wlan {
                    handle,
                    interface: guid,
                }),
                None => {
                    let _ = WlanCloseHandle(handle, None);
                    None
                }
            }
        }
    }
}

impl Drop for Wlan {
    fn drop(&mut self) {
        unsafe {
            let _ = WlanCloseHandle(self.handle, None);
        }
    }
}

/// 以 0 终止的 UTF-16 数组转字符串。
fn u16z_to_string(raw: &[u16]) -> String {
    let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
    String::from_utf16_lossy(&raw[..end])
}

/// 认证算法枚举转显示文本。
fn auth_algorithm_text(algo: DOT11_AUTH_ALGORITHM) -> &'static str {
    match algo {
        DOT11_AUTH_ALGO_80211_OPEN => "Open",
        DOT11_AUTH_ALGO_80211_SHARED_KEY => "Shared",
        DOT11_AUTH_ALGO_WPA => "WPA-Enterprise",
        DOT11_AUTH_ALGO_WPA_PSK => "WPA-Personal",
        DOT11_AUTH_ALGO_WPA_NONE => "WPA-None",
        DOT11_AUTH_ALGO_RSNA => "WPA2-Enterprise",
        DOT11_AUTH_ALGO_RSNA_PSK => "WPA2-Personal",
        DOT11_AUTH_ALGO_WPA3 => "WPA3",
        DOT11_AUTH_ALGO_WPA3_SAE => "WPA3-Personal",
        DOT11_AUTH_ALGO_OWE => "OWE",
        DOT11_AUTH_ALGO_WPA3_ENT => "WPA3-Enterprise",
        _ => "Unknown",
    }
}

/// 触发网卡主动扫描。
///
/// WLAN API 的可用网络列表来自网卡上报；网卡关联 AP 后会抑制全信道扫描，
/// 列表可能长时间只剩当前连接的网络。主动扫描让列表恢复完整。
/// 返回是否成功发起扫描请求（扫描在后台进行，调用方需等待数秒）。
pub fn trigger_active_scan() -> bool {
    unsafe {
        let Some(wlan) = Wlan::open() else {
            return false;
        };
        WlanScan(wlan.handle, &wlan.interface, None, None, None) == 0
    }
}

/// 扫描可见网络并标注已保存配置与当前连接，按信号降序。
pub fn scan() -> Result<Vec<WifiNetwork>, String> {
    unsafe {
        let Some(wlan) = Wlan::open() else {
            return Err("WlanOpenHandle failed".to_string());
        };
        let mut list: *mut WLAN_AVAILABLE_NETWORK_LIST = std::ptr::null_mut();
        if WlanGetAvailableNetworkList(wlan.handle, &wlan.interface, 0, None, &mut list) != 0 {
            return Err("WlanGetAvailableNetworkList failed".to_string());
        }
        let mut networks: Vec<WifiNetwork> = Vec::new();
        if !list.is_null() {
            let count = (*list).dwNumberOfItems as usize;
            let base = (*list).Network.as_ptr();
            for i in 0..count {
                let n = &*base.add(i);
                let ssid = ssid_to_string(&n.dot11Ssid.ucSSID[..n.dot11Ssid.uSSIDLength as usize]);
                if ssid.is_empty() {
                    continue;
                }
                // 同一 SSID 多条目合并，信号取最大值
                let signal = n.wlanSignalQuality.min(100) as u8;
                let saved = n.strProfileName[0] != 0;
                let connected = n.dwFlags & WLAN_AVAILABLE_NETWORK_CONNECTED != 0;
                if let Some(existing) = networks.iter_mut().find(|e| e.ssid == ssid) {
                    existing.signal = existing.signal.max(signal);
                    existing.saved |= saved;
                    existing.connected |= connected;
                } else {
                    networks.push(WifiNetwork {
                        ssid,
                        signal,
                        secured: n.bSecurityEnabled.as_bool(),
                        saved,
                        connected,
                    });
                }
            }
            WlanFreeMemory(list as *const core::ffi::c_void);
        }
        networks.sort_by_key(|n| std::cmp::Reverse(n.signal));
        Ok(networks)
    }
}

/// 当前连接的 SSID。
pub fn connected_ssid() -> Option<String> {
    unsafe {
        let wlan = Wlan::open()?;
        let mut data: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut size = 0u32;
        if WlanQueryInterface(
            wlan.handle,
            &wlan.interface,
            wlan_intf_opcode_current_connection,
            None,
            &mut size,
            &mut data,
            None,
        ) != 0
            || data.is_null()
        {
            return None;
        }
        let attrs = &*(data as *const WLAN_CONNECTION_ATTRIBUTES);
        let result = if attrs.isState == wlan_interface_state_connected {
            Some(u16z_to_string(&attrs.strProfileName))
        } else {
            None
        };
        WlanFreeMemory(data);
        result
    }
}

/// 当前连接详情；未连接时返回 None。
pub fn connection_info() -> Option<ConnectionInfo> {
    unsafe {
        let wlan = Wlan::open()?;
        let mut data: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut size = 0u32;
        if WlanQueryInterface(
            wlan.handle,
            &wlan.interface,
            wlan_intf_opcode_current_connection,
            None,
            &mut size,
            &mut data,
            None,
        ) != 0
            || data.is_null()
        {
            return None;
        }
        let attrs = &*(data as *const WLAN_CONNECTION_ATTRIBUTES);
        if attrs.isState != wlan_interface_state_connected {
            WlanFreeMemory(data);
            return None;
        }
        let assoc = &attrs.wlanAssociationAttributes;
        let ssid = u16z_to_string(&attrs.strProfileName);
        let auth = auth_algorithm_text(attrs.wlanSecurityAttributes.dot11AuthAlgorithm);
        let bssid = assoc.dot11Bssid;
        let signal = assoc.wlanSignalQuality.min(100) as u8;
        let rx = assoc.ulRxRate as f32 / 1000.0;
        let tx = assoc.ulTxRate as f32 / 1000.0;
        WlanFreeMemory(data);

        // 频段与信道：从 BSS 列表中当前 BSSID 的中心频率换算
        let (band, channel) = bss_band_channel(&wlan, &bssid);
        Some(ConnectionInfo {
            ssid,
            band,
            channel,
            receive_rate_mbps: rx,
            transmit_rate_mbps: tx,
            authentication: auth.to_string(),
            signal,
        })
    }
}

/// 由 BSS 中心频率（kHz）换算频段与信道号。
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

/// 当前连接的频段与信道，查询失败时返回空值。
unsafe fn bss_band_channel(wlan: &Wlan, bssid: &[u8; 6]) -> (String, u32) {
    let mut list: *mut WLAN_BSS_LIST = std::ptr::null_mut();
    if WlanGetNetworkBssList(
        wlan.handle,
        &wlan.interface,
        None,
        dot11_BSS_type_infrastructure,
        false,
        None,
        &mut list,
    ) != 0
        || list.is_null()
    {
        return (String::new(), 0);
    }
    let count = (*list).dwNumberOfItems as usize;
    let base = (*list).wlanBssEntries.as_ptr();
    let mut result = (String::new(), 0);
    for i in 0..count {
        let entry = &*base.add(i);
        if &entry.dot11Bssid == bssid {
            result = freq_to_band_channel(entry.ulChCenterFrequency);
            break;
        }
    }
    WlanFreeMemory(list as *const core::ffi::c_void);
    result
}

/// 发起连接（按 SSID 同名配置连接）。返回 Ok 仅表示系统接受了请求。
pub fn connect(ssid: &str) -> Result<(), String> {
    unsafe {
        let Some(wlan) = Wlan::open() else {
            return Err("WlanOpenHandle failed".to_string());
        };
        let profile_wide: Vec<u16> = ssid.encode_utf16().chain(std::iter::once(0)).collect();
        let params = WLAN_CONNECTION_PARAMETERS {
            wlanConnectionMode: wlan_connection_mode_profile,
            strProfile: PCWSTR(profile_wide.as_ptr()),
            pDot11Ssid: std::ptr::null_mut(),
            pDesiredBssidList: std::ptr::null_mut(),
            dot11BssType: dot11_BSS_type_infrastructure,
            dwFlags: 0,
        };
        let code = WlanConnect(wlan.handle, &wlan.interface, &params, None);
        if code == 0 {
            Ok(())
        } else {
            Err(format!("WlanConnect failed with code {code}"))
        }
    }
}

/// 断开当前 WiFi 连接。
pub fn disconnect() -> Result<(), String> {
    unsafe {
        let Some(wlan) = Wlan::open() else {
            return Err("WlanOpenHandle failed".to_string());
        };
        let code = WlanDisconnect(wlan.handle, &wlan.interface, None);
        if code == 0 {
            Ok(())
        } else {
            Err(format!("WlanDisconnect failed with code {code}"))
        }
    }
}

/// WiFi 无线电是否打开。查询失败或不存在 WiFi 无线电时返回 None。
///
/// 使用 WinRT Radio API，覆盖 RF 开关关闭、飞行模式等「WiFi 未打开」的场景。
pub fn wifi_radio_on() -> Option<bool> {
    use windows::Devices::Radios::{Radio, RadioKind, RadioState};
    let radios = Radio::GetRadiosAsync().ok()?.join().ok()?;
    let mut result = None;
    for radio in radios {
        if radio.Kind().ok() == Some(RadioKind::WiFi) {
            let on = radio.State().ok() == Some(RadioState::On);
            result = Some(result.unwrap_or(false) || on);
        }
    }
    result
}

/// 确保 WiFi 无线电打开：先经 WinRT Radio API 打开，再以 netsh 启用适配器兜底
/// （后者需要管理员权限，权限不足时静默失败）。返回操作后的打开状态。
pub fn ensure_wifi_on() -> bool {
    use windows::Devices::Radios::{Radio, RadioKind, RadioState};
    if wifi_radio_on() == Some(true) {
        return true;
    }
    if let Ok(op) = Radio::GetRadiosAsync() {
        if let Ok(radios) = op.join() {
            for radio in radios {
                if radio.Kind().ok() == Some(RadioKind::WiFi) {
                    let _ = radio.SetStateAsync(RadioState::On).map(|op| op.join());
                }
            }
        }
    }
    let _ = run_netsh_plain(&["interface", "set", "interface", "WLAN", "enable"]);
    wifi_radio_on().unwrap_or(false)
}

/// 执行 `netsh <args>` 并返回标准输出。仅用于适配器启停，输出不解析。
fn run_netsh_plain(args: &[&str]) -> Result<String, String> {
    let output = Command::new("netsh")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("failed to run netsh: {e}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freq_mapping() {
        assert_eq!(freq_to_band_channel(5745000), ("5 GHz".to_string(), 149));
        assert_eq!(freq_to_band_channel(2437000), ("2.4 GHz".to_string(), 6));
    }

    #[test]
    fn auth_text_mapping() {
        assert_eq!(
            auth_algorithm_text(DOT11_AUTH_ALGO_RSNA_PSK),
            "WPA2-Personal"
        );
        assert_eq!(auth_algorithm_text(DOT11_AUTH_ALGO_80211_OPEN), "Open");
    }

    #[test]
    fn u16z_conversion() {
        let raw: [u16; 4] = [0x32, 0x30, 0x32, 0];
        assert_eq!(u16z_to_string(&raw), "202");
    }
}
