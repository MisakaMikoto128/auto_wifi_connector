//! 互联网连通性探测。
//!
//! 向若干固定的 HTTP 探测端点发起 GET 请求，
//! 任一端点返回预期状态码即判定网络可用。
//! 实现为零依赖的最小 HTTP 客户端，仅支持探测所需的请求形式。

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const IO_TIMEOUT: Duration = Duration::from_secs(4);

/// (主机, 路径, 预期状态码)
const PROBES: &[(&str, &str, u16)] = &[
    ("connect.rom.miui.com", "/generate_204", 204),
    ("wifi.vivo.com.cn", "/generate_204", 204),
    ("www.msftconnecttest.com", "/connecttest.txt", 200),
    ("detectportal.firefox.com", "/canonical.html", 200),
];

/// 从 HTTP 响应状态行解析状态码：`HTTP/1.1 204 No Content` -> `Some(204)`。
pub fn parse_status_line(line: &str) -> Option<u16> {
    let mut parts = line.split_whitespace();
    if !parts.next()?.starts_with("HTTP/") {
        return None;
    }
    parts.next()?.parse().ok()
}

/// 向单个端点发起探测，返回是否得到预期状态码。
fn probe(host: &str, path: &str, expected_status: u16) -> bool {
    let Some(addr) = (host, 80).to_socket_addrs().ok().and_then(|mut it| it.next()) else {
        return false;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut buf = [0u8; 256];
    let Ok(n) = stream.read(&mut buf) else {
        return false;
    };
    let head = String::from_utf8_lossy(&buf[..n]);
    let Some(line) = head.lines().next() else {
        return false;
    };
    parse_status_line(line) == Some(expected_status)
}

/// 任一探测端点可达即判定网络可用。
pub fn is_online() -> bool {
    PROBES.iter().any(|(host, path, status)| probe(host, path, *status))
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
}
