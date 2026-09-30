//! WiFi 扫描、已保存配置读取与连接，基于 Windows netsh 命令。
//!
//! netsh 输出语言跟随系统区域设置，解析同时兼容英文与中文标签；
//! 输出字节按 GBK 解码，以正确读取含非 ASCII 字符的 SSID。
//! 所有子进程均带 CREATE_NO_WINDOW 标志，否则 GUI 程序中每次调用都会闪现控制台窗口。

use encoding_rs::GBK;
use serde::Serialize;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::{Duration, Instant};

/// Win32 创建进程标志：不创建控制台窗口。
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 一个扫描到的 WiFi 网络。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiNetwork {
    pub ssid: String,
    /// 信号强度，0-100。
    pub signal: u8,
    /// 是否需要密码（开放网络为 false）。
    pub secured: bool,
    /// 是否存在同名的已保存配置，存在则可直接连接。
    pub saved: bool,
    /// 是否为当前已连接的网络。
    pub connected: bool,
}

/// 执行 `netsh <args>` 并返回解码后的标准输出。
fn run_netsh_plain(args: &[&str]) -> Result<String, String> {
    let output = Command::new("netsh")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("failed to run netsh: {e}"))?;
    Ok(decode_netsh_output(&output.stdout))
}

/// 执行 `netsh wlan <args>`。
fn run_netsh(args: &[&str]) -> Result<String, String> {
    let mut full = vec!["wlan"];
    full.extend_from_slice(args);
    run_netsh_plain(&full)
}

/// 解码 netsh 输出。
///
/// netsh 输出的标签部分（SSID、Signal 等）是控制台代码页编码（中文系统为 GBK，
/// 英文系统为 ASCII），而 SSID 与配置文件名的值部分是 UTF-8 字节。
/// 整体按单一编码解码必然有一方乱码，因此按行处理：
/// 名称行的值用 UTF-8 解码（失败回退 GBK），其余内容用 GBK 解码。
fn decode_netsh_output(bytes: &[u8]) -> String {
    let mut out = String::new();
    for raw_line in bytes.split(|&b| b == b'\n') {
        let line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
        out.push_str(&decode_line(line));
        out.push('\n');
    }
    out
}

/// 解码单行。`SSID n : 名称`、`All User Profile : 名称` 等名称行的
/// 值部分按 UTF-8 解码，其余部分按 GBK 解码。
fn decode_line(line: &[u8]) -> String {
    let Some(colon) = line.iter().position(|&b| b == b':') else {
        return GBK.decode(line).0.into_owned();
    };
    let label = GBK.decode(&line[..colon]).0;
    let trimmed = label.trim();
    let is_name_line = (trimmed.starts_with("SSID") && !trimmed.contains("BSSID"))
        || trimmed.contains("User Profile")
        || trimmed.contains("用户配置文件");
    if !is_name_line {
        return GBK.decode(line).0.into_owned();
    }
    let value = &line[colon + 1..];
    let value_text = match String::from_utf8(value.to_vec()) {
        Ok(text) => text,
        Err(_) => GBK.decode(value).0.into_owned(),
    };
    format!("{label}:{value_text}")
}

/// 从行中标签与值的固定分隔位置取值：`标签 : 值`。
fn value_after_colon(line: &str) -> Option<&str> {
    line.split_once(':')
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

/// 判断一行是否为 `SSID <n> : <名称>` 形式的网络起始行，返回名称。
fn parse_ssid_header(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix("SSID ")?;
    let (num, value) = rest.split_once(':')?;
    if num.trim().chars().all(|c| c.is_ascii_digit()) && !num.trim().is_empty() {
        let ssid = value.trim();
        if !ssid.is_empty() {
            return Some(ssid.to_string());
        }
    }
    None
}

/// 解析 `netsh wlan show networks mode=bssid` 的输出，按信号降序返回网络列表。
/// 同一 SSID 的多个 BSSID 合并为一个条目，信号取最大值。
pub fn parse_networks(output: &str) -> Vec<WifiNetwork> {
    let mut networks: Vec<WifiNetwork> = Vec::new();
    for line in output.lines() {
        if let Some(ssid) = parse_ssid_header(line) {
            if !networks.iter().any(|n| n.ssid == ssid) {
                networks.push(WifiNetwork {
                    ssid,
                    signal: 0,
                    secured: true,
                    saved: false,
                    connected: false,
                });
            }
            continue;
        }
        let Some(current) = networks.last_mut() else {
            continue;
        };
        let trimmed = line.trim();
        let is_signal = trimmed.starts_with("Signal") || trimmed.starts_with("信号");
        let is_auth = trimmed.starts_with("Authentication") || trimmed.starts_with("身份验证");
        if is_signal {
            if let Some(v) = value_after_colon(trimmed) {
                if let Ok(pct) = v.trim_end_matches('%').trim().parse::<u8>() {
                    current.signal = current.signal.max(pct);
                }
            }
        } else if is_auth {
            if let Some(v) = value_after_colon(trimmed) {
                current.secured = !(v.contains("Open") || v.contains("开放"));
            }
        }
    }
    networks.sort_by_key(|n| std::cmp::Reverse(n.signal));
    networks
}

/// 解析 `netsh wlan show profiles` 的输出，返回已保存配置名称列表。
pub fn parse_profiles(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|l| {
            let t = l.trim();
            t.starts_with("All User Profile") || t.starts_with("所有用户配置文件")
        })
        .filter_map(value_after_colon)
        .map(str::to_string)
        .collect()
}

/// 解析 `netsh wlan show interfaces` 的输出，返回当前连接的 SSID。
pub fn parse_connected_ssid(output: &str) -> Option<String> {
    for line in output.lines() {
        let t = line.trim();
        if t.starts_with("SSID") && !t.contains("BSSID") {
            if let Some(rest) = t.strip_prefix("SSID") {
                // interfaces 输出中为 `SSID : 名称`，区别于扫描输出的 `SSID 1 : 名称`
                if rest.trim_start().starts_with(':') {
                    return value_after_colon(t).map(str::to_string);
                }
            }
        }
    }
    None
}

/// 扫描可见网络，并标注已保存配置与当前连接。
pub fn scan() -> Result<Vec<WifiNetwork>, String> {
    let mut networks = parse_networks(&run_netsh(&["show", "networks", "mode=bssid"])?);
    let profiles = parse_profiles(&run_netsh(&["show", "profiles"])?);
    let connected = parse_connected_ssid(&run_netsh(&["show", "interfaces"])?);
    for n in &mut networks {
        n.saved = profiles.iter().any(|p| p == &n.ssid);
        n.connected = connected.as_deref() == Some(n.ssid.as_str());
    }
    networks.sort_by_key(|n| std::cmp::Reverse(n.signal));
    Ok(networks)
}

/// 当前连接的 SSID。
pub fn connected_ssid() -> Option<String> {
    parse_connected_ssid(&run_netsh(&["show", "interfaces"]).ok()?)
}

/// 发起连接。返回 Ok 仅表示 netsh 接受了请求，不代表已关联成功。
pub fn connect(ssid: &str) -> Result<(), String> {
    if ssid.contains('"') {
        return Err("ssid must not contain quotes".to_string());
    }
    let out = run_netsh(&["connect", &format!("name={ssid}")])?;
    if out.contains("success") || out.contains("成功") {
        Ok(())
    } else {
        Err(out.trim().to_string())
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

/// 断开当前 WiFi 连接。
pub fn disconnect() -> Result<(), String> {
    run_netsh(&["disconnect"]).map(|_| ())
}

/// 等待网卡关联到指定 SSID，超时返回 false。
pub fn wait_associated(ssid: &str, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if connected_ssid().as_deref() == Some(ssid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(800));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const NETWORKS_EN: &str = r#"
Interface name : WLAN
There are 2 networks currently visible.

SSID 1 : CMCC-u2c6-5G
    Network type            : Infrastructure
    Authentication          : WPA2-Personal
    Encryption              : CCMP
    BSSID 1                 : 24:e8:e5:7a:6b:33
         Signal             : 79%
         Radio type         : 802.11ac

SSID 2 : CafeGuest
    Network type            : Infrastructure
    Authentication          : Open
    Encryption              : None
    BSSID 1                 : aa:bb:cc:dd:ee:ff
         Signal             : 45%
    BSSID 2                 : 11:22:33:44:55:66
         Signal             : 62%
"#;

    const NETWORKS_ZH: &str = r#"
接口名称 : WLAN
当前有 1 个网络可见。

SSID 1 : 魔法上网
    网络类型              : 结构
    身份验证              : WPA2 - 个人
    加密                  : CCMP
    BSSID 1               : 24:e8:e5:7a:6b:33
         信号               : 66%
"#;

    const PROFILES_EN: &str = r#"
Profiles on interface WLAN:

Group policy profiles (read only)
---------------------------------
    <None>

User profiles
-------------
    All User Profile     : CMCC-u2c6-5G
    All User Profile     : 魔法上网
    All User Profile     : Redmi K70
"#;

    const PROFILES_ZH: &str = r#"
接口 WLAN 上的配置文件:

用户配置文件
-------------
    所有用户配置文件 : CMCC-u2c6-5G
    所有用户配置文件 : 抛瓦拉满_5G.
"#;

    const INTERFACES_CONNECTED: &str = r#"
There is 1 interface on the system:

    Name                   : WLAN
    State                  : connected
    SSID                   : CMCC-u2c6-5G
    AP BSSID               : 24:e8:e5:7a:6b:33
    Signal                 : 79%
"#;

    const INTERFACES_DISCONNECTED: &str = r#"
There is 1 interface on the system:

    Name                   : WLAN
    State                  : disconnected
"#;

    #[test]
    fn decodes_utf8_ssid_value_with_ascii_label() {
        // 真实样本：英文系统 netsh 输出，SSID 值为 UTF-8 字节（「耿」= E8 80 BF）
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"SSID 5 : ");
        bytes.extend_from_slice("耿".as_bytes());
        bytes.extend_from_slice(b"\r\n");
        let text = decode_netsh_output(&bytes);
        assert!(text.contains("SSID 5 : 耿"), "got: {text:?}");
    }

    #[test]
    fn decodes_utf8_value_with_gbk_label() {
        // 模拟中文系统：标签「所有用户配置文件」为 GBK 字节，值「魔法上网」为 UTF-8 字节
        let (label, _, _) = GBK.encode("    所有用户配置文件 ");
        let mut bytes = label.into_owned();
        bytes.extend_from_slice(b": ");
        bytes.extend_from_slice("魔法上网".as_bytes());
        bytes.extend_from_slice(b"\r\n");
        let text = decode_netsh_output(&bytes);
        assert!(text.contains("所有用户配置文件"), "label lost: {text:?}");
        assert!(text.contains("魔法上网"), "value lost: {text:?}");
    }

    #[test]
    fn decodes_gbk_value_when_not_utf8() {
        // 值为 GBK 字节时回退 GBK 解码
        let (value, _, _) = GBK.encode("中文热点");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"SSID 1 : ");
        bytes.extend_from_slice(&value);
        bytes.extend_from_slice(b"\r\n");
        let text = decode_netsh_output(&bytes);
        assert!(text.contains("中文热点"), "got: {text:?}");
    }

    #[test]
    fn parses_profiles_from_raw_utf8_name_bytes() {
        // 真实样本：profiles 输出的配置名是 UTF-8 字节，经解码后应提取正确中文名
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"User profiles\r\n-------------\r\n    All User Profile     : ");
        bytes.extend_from_slice("3D打印机".as_bytes());
        bytes.extend_from_slice(b"\r\n    All User Profile     : ");
        bytes.extend_from_slice("抛瓦拉满_5G.".as_bytes());
        bytes.extend_from_slice(b"\r\n");
        let profiles = parse_profiles(&decode_netsh_output(&bytes));
        assert_eq!(profiles, vec!["3D打印机", "抛瓦拉满_5G."]);
    }

    #[test]
    fn parses_english_networks() {
        let nets = parse_networks(NETWORKS_EN);
        assert_eq!(nets.len(), 2);
        // 按信号降序：CMCC 79 在前
        assert_eq!(nets[0].ssid, "CMCC-u2c6-5G");
        assert_eq!(nets[0].signal, 79);
        assert!(nets[0].secured);
        // 同一 SSID 多个 BSSID 合并，信号取最大值
        assert_eq!(nets[1].ssid, "CafeGuest");
        assert_eq!(nets[1].signal, 62);
        assert!(!nets[1].secured);
    }

    #[test]
    fn parses_chinese_networks() {
        let nets = parse_networks(NETWORKS_ZH);
        assert_eq!(nets.len(), 1);
        assert_eq!(nets[0].ssid, "魔法上网");
        assert_eq!(nets[0].signal, 66);
        assert!(nets[0].secured);
    }

    #[test]
    fn parses_english_profiles() {
        let profiles = parse_profiles(PROFILES_EN);
        assert_eq!(profiles, vec!["CMCC-u2c6-5G", "魔法上网", "Redmi K70"]);
    }

    #[test]
    fn parses_chinese_profiles() {
        let profiles = parse_profiles(PROFILES_ZH);
        assert_eq!(profiles, vec!["CMCC-u2c6-5G", "抛瓦拉满_5G."]);
    }

    #[test]
    fn parses_connected_ssid() {
        assert_eq!(
            parse_connected_ssid(INTERFACES_CONNECTED).as_deref(),
            Some("CMCC-u2c6-5G")
        );
    }

    #[test]
    fn parses_disconnected_state() {
        assert_eq!(parse_connected_ssid(INTERFACES_DISCONNECTED), None);
    }

    #[test]
    fn rejects_ssid_with_quotes() {
        assert!(connect("bad\"ssid").is_err());
    }
}
