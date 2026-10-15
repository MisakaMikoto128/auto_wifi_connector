#!/usr/bin/env python3
"""diagnose.py — 断网问题诊断数据采集。

在出现问题的电脑上运行本脚本，将生成的 diagnose-report.txt 发回。
报告包含 netsh 原始输出的字节级记录与系统环境信息。

用法:  python diagnose.py
输出:  diagnose-report.txt（与本脚本同目录）
"""

import locale
import platform
import subprocess
from datetime import datetime
from pathlib import Path

REPORT_FILE = Path(__file__).with_name("diagnose-report.txt")

COMMANDS = [
    ["netsh", "wlan", "show", "interfaces"],
    ["netsh", "wlan", "show", "networks", "mode=bssid"],
    ["netsh", "wlan", "show", "profiles"],
]


def run_command(cmd: list[str]) -> dict:
    """执行命令并返回原始字节、退出码与两种编码的解码结果。"""
    result = subprocess.run(cmd, capture_output=True, timeout=30)
    raw = result.stdout
    return {
        "returncode": result.returncode,
        "raw_hex_preview": raw[:400].hex(" "),
        "as_utf8": raw.decode("utf-8", errors="replace"),
        "as_gbk": raw.decode("gbk", errors="replace"),
    }


def main() -> None:
    lines = [
        f"diagnose report — {datetime.now():%Y-%m-%d %H:%M:%S}",
        f"windows: {platform.version()} ({platform.release()})",
        f"system locale: {locale.getpreferredencoding()}",
        "",
    ]
    for cmd in COMMANDS:
        lines.append("=" * 60)
        lines.append(f"$ {' '.join(cmd)}")
        try:
            result = run_command(cmd)
            lines.append(f"exit code: {result['returncode']}")
            lines.append(f"raw bytes (first 400, hex): {result['raw_hex_preview']}")
            lines.append("--- decoded as utf-8 ---")
            lines.append(result["as_utf8"])
            lines.append("--- decoded as gbk ---")
            lines.append(result["as_gbk"])
        except Exception as e:
            lines.append(f"command failed: {type(e).__name__}: {e}")
        lines.append("")

    REPORT_FILE.write_text("\n".join(lines), encoding="utf-8")
    print(f"report written to {REPORT_FILE}")
    print("请将该文件内容发回")


if __name__ == "__main__":
    main()
