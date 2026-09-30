#!/usr/bin/env python3
"""keepalive.py — 断网保底脚本。

开发测试期间在后台运行。周期性检测互联网连通性，
一旦判定断网，自动重连到启动时记录的 WiFi 网络。

用法:  python keepalive.py [SSID]
       不传入 SSID 时，自动读取当前已连接网络的 SSID。
日志:  keepalive.log (UTF-8)
"""

import subprocess
import sys
import time
import urllib.request
from datetime import datetime
from pathlib import Path

CHECK_INTERVAL = 15          # 连通性检测周期（秒）
HTTP_TIMEOUT = 5             # 单次 HTTP 探测超时（秒）
RECONNECT_WAIT = 20          # 发起重连后等待关联完成的时长（秒）
LOG_FILE = Path(__file__).with_name("keepalive.log")

# 任一探测端点返回预期结果即判定网络可用
PROBE_ENDPOINTS = [
    ("http://connect.rom.miui.com/generate_204", 204),
    ("http://wifi.vivo.com.cn/generate_204", 204),
    ("http://www.msftconnecttest.com/connecttest.txt", 200),
    ("http://detectportal.firefox.com/canonical.html", 200),
]


def log(message: str) -> None:
    line = f"[{datetime.now():%Y-%m-%d %H:%M:%S}] {message}"
    with LOG_FILE.open("a", encoding="utf-8") as fp:
        fp.write(line + "\n")
    print(line, flush=True)


def run_netsh(*args: str) -> str:
    result = subprocess.run(
        ["netsh", "wlan", *args],
        capture_output=True, timeout=30,
    )
    return result.stdout.decode("gbk", errors="replace")


def current_ssid() -> str | None:
    """返回当前已连接网络的 SSID；未连接时返回 None。"""
    for line in run_netsh("show", "interfaces").splitlines():
        stripped = line.strip()
        if stripped.startswith("SSID") and "BSSID" not in stripped:
            return stripped.split(":", 1)[1].strip() or None
    return None


def is_online() -> bool:
    """综合在线判定：WiFi 已关联且任一探测端点返回预期状态码。

    仅做 HTTP 探测会被本机代理虚拟网卡（TUN 模式）误导：
    WiFi 物理断开后虚拟网卡仍可能响应探测。
    """
    if current_ssid() is None:
        return False
    for url, expected_status in PROBE_ENDPOINTS:
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "keepalive/1.0"})
            with urllib.request.urlopen(req, timeout=HTTP_TIMEOUT) as resp:
                if resp.status == expected_status:
                    return True
        except Exception:
            continue
    return False


def reconnect(ssid: str) -> None:
    """断开当前连接并重新连接到指定 SSID。"""
    log(f"reconnecting to {ssid!r}")
    run_netsh("disconnect")
    time.sleep(3)
    run_netsh("connect", f"name={ssid}")
    time.sleep(RECONNECT_WAIT)


def main() -> None:
    ssid = sys.argv[1] if len(sys.argv) > 1 else current_ssid()
    if not ssid:
        log("no SSID given and no network currently connected; exit")
        sys.exit(1)

    log(f"keepalive started, target SSID={ssid!r}, interval={CHECK_INTERVAL}s")
    while True:
        if is_online():
            log("online")
        else:
            log("OFFLINE detected")
            reconnect(ssid)
            log("online after reconnect" if is_online() else "still offline, will retry")
        time.sleep(CHECK_INTERVAL)


if __name__ == "__main__":
    main()
