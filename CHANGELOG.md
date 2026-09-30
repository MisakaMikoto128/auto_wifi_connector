# 变更日志

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循[语义化版本](https://semver.org/lang/zh-CN/)。

## [0.1.1] - 2026-09-30

### 新增

- GitHub Actions：CI（fmt、clippy、test）与 tag 触发的自动 Release 流水线
- Issue 模板（缺陷报告、功能建议）与 Pull Request 模板
- CONTRIBUTING、CODE_OF_CONDUCT、SECURITY 文档
- dependabot 依赖更新配置

### 修复

- 清理全部 clippy 警告，统一使用 `sort_by_key` 降序排序

## [0.1.0] - 2026-09-30

### 新增

- 连通性探测：HTTP 204/200 多端点探测结合 WiFi 关联状态综合判定，探测时间有界
- 自动断网重连轮询：候选网络（可见且已保存配置）按信号降序依次连接验证，循环至恢复
- 报刊风格界面：WiFi 名录、恢复过程动画、滚动日志
- 系统托盘：关窗隐藏、右键菜单「显示窗口/退出」
- 开机自启，可在界面中关闭
- 单元测试 12 项与真实断网重连集成测试
- 独立保底脚本 tools/keepalive.py
- GitHub Pages 项目页

[0.1.1]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/MisakaMikoto128/auto_wifi_connector/releases/tag/v0.1.0
