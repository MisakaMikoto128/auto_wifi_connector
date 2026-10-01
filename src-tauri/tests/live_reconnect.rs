//! 真实断网重连集成测试：断开当前 WiFi，再走完整重连链路恢复。
//! 执行期间会真实断网约 20 秒，需手动运行：
//!
//! ```sh
//! cargo test --test live_reconnect -- --ignored --nocapture
//! ```

use auto_wifi_connector_lib::{netcheck, wifi};
use std::time::Duration;

/// WiFi 无线电状态检测：当前 WiFi 开启，应返回 Some(true)。
/// CI 运行在无 WiFi 硬件的虚拟机上，故标记为手动运行。
#[test]
#[ignore]
fn wifi_radio_reports_on() {
    assert_eq!(wifi::wifi_radio_on(), Some(true));
}

#[test]
#[ignore]
fn reconnect_after_disconnect() {
    let ssid = wifi::connected_ssid().expect("test requires a connected wifi network");
    assert!(
        netcheck::is_online(),
        "test requires working internet before start"
    );
    println!("current ssid: {ssid}");

    wifi::disconnect().expect("disconnect failed");
    std::thread::sleep(Duration::from_secs(3));
    assert_ne!(
        wifi::connected_ssid().as_deref(),
        Some(ssid.as_str()),
        "should be disconnected after netsh disconnect"
    );
    println!("disconnected ok");

    wifi::connect(&ssid).expect("connect request failed");
    assert!(
        wifi::wait_associated(&ssid, Duration::from_secs(15)),
        "should re-associate within 15s"
    );
    println!("re-associated ok");

    assert!(
        netcheck::is_online_retry(3, Duration::from_secs(2)),
        "internet should be reachable after reconnect"
    );
    println!("online ok");
}

/// 扫描并打印当前可见网络，人工核对中文 SSID 无乱码。
#[test]
#[ignore]
fn scan_prints_networks() {
    for n in wifi::scan().expect("scan failed") {
        println!("ssid={:?} signal={} saved={}", n.ssid, n.signal, n.saved);
    }
}

/// 真实连通性探测：当前网络可用，应判定在线（含 portal 欺骗防御逻辑）。
#[test]
#[ignore]
fn real_probe_is_online() {
    assert!(netcheck::is_online(), "should be online");
}
