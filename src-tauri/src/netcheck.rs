//! 互联网连通性探测。
//!
//! 向若干固定的 HTTP 探测端点发起 GET 请求，
//! 任一端点返回预期结果即判定网络可用。
//! 实现为零依赖的最小 HTTP 客户端，仅支持探测所需的请求形式。
//!
//! 防 captive portal 欺骗：内网 portal 会劫持所有 HTTP 请求并返回 200 登录页，
//! 因此 204 端点要求状态码恰为 204（portal 不会返回 204），
//! 200 端点额外校验响应正文片段。

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const IO_TIMEOUT: Duration = Duration::from_secs(4);
/// 单次读取的响应上限，状态行、响应头与正文开头足以容纳。
const READ_LIMIT: usize = 4096;

/// 一个探测端点。
struct Probe {
    host: &'static str,
    path: &'static str,
    expected_status: u16,
    /// 预期正文片段。204 端点无正文为 None；200 端点必须匹配，防止 portal 劫持冒充。
    expected_body: Option<&'static str>,
}

const PROBES: &[Probe] = &[
    Probe {
        host: "connect.rom.miui.com",
        path: "/generate_204",
        expected_status: 204,
        expected_body: None,
    },
    Probe {
        host: "wifi.vivo.com.cn",
        path: "/generate_204",
        expected_status: 204,
        expected_body: None,
    },
    Probe {
        host: "connectivitycheck.platform.hicloud.com",
        path: "/generate_204",
        expected_status: 204,
        expected_body: None,
    },
    Probe {
        host: "www.msftconnecttest.com",
        path: "/connecttest.txt",
        expected_status: 200,
        expected_body: Some("Microsoft Connect Test"),
    },
    Probe {
        host: "captive.apple.com",
        path: "/hotspot-detect.html",
        expected_status: 200,
        expected_body: Some("Success"),
    },
];

/// 从 HTTP 响应状态行解析状态码：`HTTP/1.1 204 No Content` -> `Some(204)`。
pub fn parse_status_line(line: &str) -> Option<u16> {
    let mut parts = line.split_whitespace();
    if !parts.next()?.starts_with("HTTP/") {
        return None;
    }
    parts.next()?.parse().ok()
}

/// 校验完整响应：状态码匹配，且当端点要求时正文包含预期片段。
/// 输入为原始响应字节（状态行 + 头 + 正文开头）。
pub fn response_matches(raw: &[u8], expected_status: u16, expected_body: Option<&str>) -> bool {
    let Some(head_end) = raw.windows(2).position(|w| w == b"\r\n") else {
        return false;
    };
    let Ok(status_line) = std::str::from_utf8(&raw[..head_end]) else {
        return false;
    };
    if parse_status_line(status_line) != Some(expected_status) {
        return false;
    }
    let Some(expected) = expected_body else {
        return true;
    };
    // 正文在头部之后（\r\n\r\n 分隔）；直接在整个响应中查找片段，
    // 状态行与响应头不含正文内容，不会误匹配
    let text = String::from_utf8_lossy(raw);
    text.contains(expected)
}

/// 向单个端点发起探测，返回是否得到预期结果。
fn probe(p: &Probe) -> bool {
    let Some(addr) = (p.host, 80)
        .to_socket_addrs()
        .ok()
        .and_then(|mut it| it.next())
    else {
        return false;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        p.path, p.host
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut buf = Vec::with_capacity(READ_LIMIT);
    let mut chunk = [0u8; 1024];
    while buf.len() < READ_LIMIT {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    response_matches(&buf, p.expected_status, p.expected_body)
}

/// 任一探测端点可达即判定网络可用。
pub fn is_online() -> bool {
    PROBES.iter().any(probe)
}

/// 时间有界的连通性判定。
/// DNS 解析在网卡切换后可能长时间阻塞（超过分钟级），
/// 因此将探测放入独立线程并以超时等待结果，保证调用方有界返回。
pub fn is_online_bounded(timeout: Duration) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(is_online());
    });
    rx.recv_timeout(timeout).unwrap_or(false)
}

/// 带重试的连通性判定，用于 WiFi 刚关联后等待网络就绪。
pub fn is_online_retry(attempts: u32, interval: Duration) -> bool {
    for i in 0..attempts {
        if is_online_bounded(Duration::from_secs(12)) {
            return true;
        }
        if i + 1 < attempts {
            std::thread::sleep(interval);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_lines() {
        assert_eq!(parse_status_line("HTTP/1.1 204 No Content"), Some(204));
        assert_eq!(parse_status_line("HTTP/1.0 200 OK"), Some(200));
        assert_eq!(parse_status_line("HTTP/2 301"), Some(301));
    }

    #[test]
    fn rejects_invalid_status_lines() {
        assert_eq!(parse_status_line(""), None);
        assert_eq!(parse_status_line("GET / HTTP/1.1"), None);
        assert_eq!(parse_status_line("HTTP/1.1 abc"), None);
        assert_eq!(parse_status_line("204 No Content"), None);
    }

    #[test]
    fn accepts_204_endpoint() {
        let raw = b"HTTP/1.1 204 No Content\r\nServer: nginx\r\n\r\n";
        assert!(response_matches(raw, 204, None));
    }

    #[test]
    fn accepts_200_endpoint_with_expected_body() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 22\r\n\r\nMicrosoft Connect Test";
        assert!(response_matches(raw, 200, Some("Microsoft Connect Test")));
    }

    #[test]
    fn rejects_portal_hijack_on_204_endpoint() {
        // portal 劫持：对 generate_204 请求返回 200 登录页
        let raw =
            b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body>Login</body></html>";
        assert!(!response_matches(raw, 204, None));
    }

    #[test]
    fn rejects_portal_hijack_on_200_endpoint() {
        // portal 劫持：状态码 200 但正文是登录页而非预期内容
        let raw =
            b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n<html><body>Login</body></html>";
        assert!(!response_matches(raw, 200, Some("Microsoft Connect Test")));
    }

    #[test]
    fn rejects_wrong_status() {
        let raw = b"HTTP/1.1 302 Found\r\nLocation: http://portal.example/\r\n\r\n";
        assert!(!response_matches(raw, 204, None));
        assert!(!response_matches(raw, 200, Some("Success")));
    }
}
