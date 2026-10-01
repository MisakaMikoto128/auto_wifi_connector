# 变更日志

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循[语义化版本](https://semver.org/lang/zh-CN/)。

## [0.2.4] - 2026-10-01

### 修复

- SSID 含空格时连接失败：connect 的 name 参数补充引号，避免被 netsh 拆成多个参数
- 用户在界面关闭开机自启后，重启程序会被重新开启：改为仅首次运行时默认注册，之后尊重用户选择

### 新增

- 单实例运行：重复启动时聚焦已有窗口，不再产生第二个进程
- 托盘图标左键单击显示窗口

## [0.2.3] - 2026-10-01

### 修复

- 内网 captive portal 环境下误判在线：portal 劫持 HTTP 请求返回 200 登录页，原探测逻辑仅凭状态码判定会被欺骗。204 端点要求状态码恰为 204，200 端点额外校验响应正文片段；探测端点调整为 miui/vivo/华为三个 204 与微软、苹果两个带正文校验的 200

## [0.2.2] - 2026-10-01

### 新增

- Release 附带 SHA256 校验文件
- README 与项目页增加安装说明：Windows SmartScreen 提示的处理方法与完整性校验

## [0.2.1] - 2026-09-30

### 修复

- 中文等非 ASCII 的 WiFi 名称显示乱码：netsh 输出的标签部分是控制台代码页编码、名称值部分是 UTF-8 字节，解码改为按行识别名称行，值用 UTF-8（回退 GBK），其余用 GBK

## [0.2.0] - 2026-09-30

### 新增

- WiFi 无线电关闭（RF 开关、飞行模式）时自动尝试打开：WinRT Radio API 设置无线电状态，netsh 启用适配器兜底（需要管理员权限，失败时在界面日志提示手动打开）

### 修复

- **P0**：GUI 程序每次调用 netsh 都会闪现控制台窗口并抢夺焦点。所有子进程补充 `CREATE_NO_WINDOW` 标志
- 修复连通性探测的误判：本机代理虚拟网卡（TUN 模式）在 WiFi 物理断开后仍响应 HTTP 探测，改为「WiFi 已关联 且 互联网可达」综合判定

注意：v0.1.0 与 v0.1.1 因上述 P0 缺陷已撤回，请勿使用。

## [0.1.1] - 2026-09-30（已撤回）

### 新增

- GitHub Actions：CI（fmt、clippy、test）与 tag 触发的自动 Release 流水线
- Issue 模板（缺陷报告、功能建议）与 Pull Request 模板
- CONTRIBUTING、CODE_OF_CONDUCT、SECURITY 文档
- dependabot 依赖更新配置

### 修复

- 清理全部 clippy 警告，统一使用 `sort_by_key` 降序排序

## [0.1.0] - 2026-09-30（已撤回）

### 新增

- 连通性探测：HTTP 204/200 多端点探测结合 WiFi 关联状态综合判定，探测时间有界
- 自动断网重连轮询：候选网络（可见且已保存配置）按信号降序依次连接验证，循环至恢复
- 报刊风格界面：WiFi 名录、恢复过程动画、滚动日志
- 系统托盘：关窗隐藏、右键菜单「显示窗口/退出」
- 开机自启，可在界面中关闭
- 单元测试 12 项与真实断网重连集成测试
- 独立保底脚本 tools/keepalive.py
- GitHub Pages 项目页

[0.2.4]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/bea81fa...v0.2.0

