#!/usr/bin/env python3
"""portal-probe.py — captive portal 环境下的带外遥测可行性探测。

在「WiFi 已关联但无互联网」的内网/portal 环境中运行本脚本，
逐项检测哪些通道仍然可用，用于评估 DNS 隧道遥测的可行性。

用法:  python portal-probe.py [隧道测试域名]
       隧道测试域名示例：任意 dnslog 子域名，如 abc123.dnslog.cn。
       不传入时跳过隧道实证，仅做通道检测。

检测项:
  1. WiFi 关联状态
  2. HTTP 连通性与劫持识别（204 端点应返回 204，200 端点应返回预期正文）
  3. 系统 DNS 解析是否被劫持（知名域名解析结果是否为真实公网地址）
  4. 直接 UDP 53 查询公共递归 DNS（绕过系统 DNS）
  5. 隧道实证：查询测试域名，需到 dnslog 页面人工核对是否到达
"""

import socket
import struct
import subprocess
import sys
import urllib.request

HTTP_TIMEOUT = 5
DNS_TIMEOUT = 4

PROBE_ENDPOINTS = [
    ("http://connect.rom.miui.com/generate_204", 204, None),
    ("http://wifi.vivo.com.cn/generate_204", 204, None),
    ("http://www.msftconnecttest.com/connecttest.txt", 200, "Microsoft Connect Test"),
    ("http://captive.apple.com/hotspot-detect.html", 200, "Success"),
]

# 公共递归 DNS，任一可达即可
PUBLIC_RESOLVERS = ["223.5.5.5", "223.6.6.6", "119.29.29.29"]


def section(title: str) -> None:
    print(f"\n== {title} ==")


def check_wifi() -> str | None:
    out = subprocess.run(
        ["netsh", "wlan", "show", "interfaces"], capture_output=True
    ).stdout.decode("gbk", errors="replace")
    for line in out.splitlines():
        t = line.strip()
        if t.startswith("SSID") and "BSSID" not in t:
            ssid = t.split(":", 1)[1].strip()
            print(f"WiFi 已关联: {ssid!r}")
            return ssid
    print("WiFi 未关联")
    return None


def check_http() -> bool:
    """任一 HTTP 探测返回预期结果即为在线；全部被劫持/失败则离线。"""
    online = False
    for url, status, body_fragment in PROBE_ENDPOINTS:
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "portal-probe/1.0"})
            with urllib.request.urlopen(req, timeout=HTTP_TIMEOUT) as resp:
                body = resp.read(512).decode("utf-8", errors="replace")
                ok = resp.status == status and (
                    body_fragment is None or body_fragment in body
                )
                print(f"  {url} -> {resp.status}, 内容符合预期: {ok}")
                online = online or ok
        except Exception as e:
            print(f"  {url} -> 失败: {type(e).__name__}")
    print(f"HTTP 判定: {'在线' if online else '离线（或被 portal 劫持）'}")
    return online


def build_dns_query(name: str, qtype: int = 1) -> bytes:
    """构造最小 DNS 查询包（A 记录）。"""
    query = struct.pack(">HHHHHH", 0x1234, 0x0100, 1, 0, 0, 0)
    for part in name.split("."):
        query += bytes([len(part)]) + part.encode()
    return query + b"\x00" + struct.pack(">HH", qtype, 1)


def dns_query_via(server: str, name: str) -> list[str] | None:
    """向指定递归服务器直接发 UDP 53 查询，返回解析到的地址列表。"""
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
            sock.settimeout(DNS_TIMEOUT)
            sock.sendto(build_dns_query(name), (server, 53))
            data, _ = sock.recvfrom(1024)
        answers = struct.unpack(">H", data[6:8])[0]
        # 跳过报文头与问题段：问题段 = 各标签 + 结尾 0 + QTYPE + QCLASS
        pos = 12
        while data[pos] != 0:
            pos += 1 + data[pos]
        pos += 5
        addrs = []
        for _ in range(answers):
            # 名称字段：指针（2 字节）或标签序列
            if data[pos] & 0xC0 == 0xC0:
                pos += 2
            else:
                while data[pos] != 0:
                    pos += 1 + data[pos]
                pos += 1
            rtype, _rclass, _ttl, rdlen = struct.unpack(">HHIH", data[pos : pos + 10])
            pos += 10
            if rtype == 1 and rdlen == 4:
                addrs.append(".".join(str(b) for b in data[pos : pos + 4]))
            pos += rdlen
        return addrs
    except Exception as e:
        print(f"  向 {server} 查询失败: {type(e).__name__}")
        return None


def check_dns_hijack() -> None:
    """对比系统解析与公共递归解析，判断 DNS 是否被劫持。"""
    test_name = "www.msftconnecttest.com"
    try:
        sys_results = {ai[4][0] for ai in socket.getaddrinfo(test_name, 80)}
        print(f"  系统 DNS 解析 {test_name}: {sorted(sys_results)}")
    except Exception as e:
        print(f"  系统 DNS 解析失败: {type(e).__name__}")
        sys_results = set()
    for server in PUBLIC_RESOLVERS:
        direct = dns_query_via(server, test_name)
        if direct is not None:
            print(f"  直接查询 {server}: {sorted(set(direct))}")
            if sys_results and direct and not sys_results & set(direct):
                print("  结论: 系统 DNS 与公共递归结果不一致，DNS 可能被劫持")
            elif sys_results & set(direct):
                print("  结论: 系统 DNS 与公共递归结果一致")
            return
    print("  结论: UDP 53 到公共递归全部被阻断")


def check_tunnel(domain: str) -> None:
    import time

    token = f"portal-probe-{int(time.time())}"
    fqdn = f"{token}.{domain}"
    print(f"  查询 {fqdn}")
    print("  请到 dnslog 页面刷新，确认是否出现该记录")
    for server in PUBLIC_RESOLVERS:
        if dns_query_via(server, fqdn) is not None:
            print(f"  已经由 {server} 发出查询")
            return
    print("  所有公共递归均不可达，隧道不可用")


def main() -> None:
    section("1. WiFi 关联状态")
    check_wifi()

    section("2. HTTP 连通性与劫持识别")
    check_http()

    section("3. DNS 劫持检测")
    check_dns_hijack()

    if len(sys.argv) > 1:
        section("4. DNS 隧道实证")
        check_tunnel(sys.argv[1])
    else:
        section("4. DNS 隧道实证")
        print("  未提供测试域名，跳过。用法: python portal-probe.py <dnslog子域名>")


if __name__ == "__main__":
    main()
