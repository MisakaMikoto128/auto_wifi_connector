//! 真实断网重连集成测试：断开当前 WiFi，再走完整重连链路恢复。
//! 执行期间会真实断网约 20 秒，需手动运行：
//!
//! ```sh
//! cargo test --test live_reconnect -- --ignored --nocapture
//! ```

use auto_wifi_connector_lib::{netcheck, wifi};
use std::time::Duration;

#[test]
#[ignore]
fn reconnect_after_disconnect() {
    let ssid = wifi::connected_ssid().expect("test requires a connected wifi network");
    assert!(netcheck::is_online(), "test requires working internet before start");
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
