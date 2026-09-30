# 安全政策

## 支持的版本

仅最新发布的版本接受安全修复。

## 报告安全问题

请通过 GitHub 的 [Security Advisories](../../security/advisories/new) 私下报告，不要在公开的 Issue 中描述安全问题的细节。

报告后会在 7 天内给出答复。

## 说明

本程序通过 `netsh` 命令操作 WiFi 连接，不读取、不传输 WiFi 密码；网络探测仅向固定的 HTTP 探测端点发起 GET 请求，不发送任何用户数据。
