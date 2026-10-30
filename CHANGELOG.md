# 变更日志

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循[语义化版本](https://semver.org/lang/zh-CN/)。

## [0.6.0] - 2026-10-30

### 新增

- 软件内更新：页脚「检查更新」与托盘「检查更新」菜单可检测是否最新版本，发现新版本时公告条一键下载安装，下载进度实时显示。更新源为本仓库 GitHub Releases（`latest.json` 清单），更新包经 minisign 签名校验，Windows 静默安装（passive）后自动重启；发布流水线新增签名密钥校验与更新清单生成

## [0.5.2] - 2026-10-30

### 修复

- 开机自启启动时弹出主窗口：主窗口改为默认隐藏，自启注册项补充 `--silent` 启动参数；开机自启启动时仅在托盘后台待命，手动启动才显示界面。旧的自启注册项在新版本首次运行时自动更新为带参数的新项

## [0.5.1] - 2026-10-24

### 变更

- Linux 实现从 nmcli 文本解析切换为 [nmrs](https://crates.io/crates/nmrs)（MIT）的 NetworkManager D-Bus API：扫描、信号、已保存配置（known）、当前连接（is_active）、连接详情、连接与断开全部来自该库，消除 CLI 输出解析；README 注明引用来源

## [0.5.0] - 2026-10-16

### 新增

- 全平台支持：macOS（CoreWLAN，Apple 官方 Wi-Fi 框架）与 Linux（NetworkManager nmcli）
- Wi-Fi 数据层重组为平台抽象层：统一接口 + 编译期平台实现（windows.rs / macos.rs / linux.rs），新增平台只需增加一个实现模块
- CI 扩展为三平台矩阵（Windows / macOS / Linux 各自执行 fmt、clippy、test）
- Release 产物扩展：Windows NSIS 安装包与便携 zip、macOS app 与 dmg（含 Apple Silicon 与 Intel）、Linux deb 与 rpm 与 AppImage

## [0.4.0] - 2026-10-16

### 变更

- WiFi 数据层从 netsh 文本解析切换为 Windows WLAN API（wlanapi.dll）结构体：扫描、信号强度、已保存配置标注、当前连接、连接详情、连接与断开全部来自系统接口，与系统显示语言和区域编码无关。调研确认：wifiscanner 的 Windows 实现同样是 netsh 文本解析且更简陋，wifi-rs 需要密码明文，均无现成库覆盖本场景，平台原生 API 是唯一可靠方案
- wifi 模块接口（scan / connect / disconnect / connected_ssid / connection_info）作为平台抽象层，后续 macOS（CoreWLAN）与 Linux（NetworkManager）接入同一接口

## [0.3.1] - 2026-10-10

### 修复

- WiFi 列表周期刷新此前被自动重连开关门控，未开启开关时列表不刷新：改为独立于开关始终执行
- 列表长时间只剩当前连接的网络：被动扫描缓存滞后（网卡关联后抑制全信道扫描），周期触发 WlanScan 主动扫描
- 「自动断网重连轮询」开关状态重启后丢失：持久化到配置目录 settings.json，启动时恢复

### 变更

- 移除待命状态的高斯三维分布曲面与示波器波形动画
- README 与项目页截图更新为当前界面

### 新增

- 页脚开源仓库链接（点击在系统浏览器打开）

## [0.3.0] - 2026-10-10

### 新增

- WiFi 名录：行内「连接」按钮手动切换到已存密码的网络；标题行刷新按钮手动重扫；状态条显示当前连接的频段、信道与速率
- 程序首次运行时自动移除 exe 的网络来源标记（MOTW），开机自启不再出现「无法验证发布者」的 Windows 授权框
- Release 附 SHA256 校验文件（自 0.2.2 起，本次文档正式说明）

### 修复

- 开机自启在版本升级后仍指向老程序：自启已开启时每次启动重新注册为当前 exe 路径；用户在界面关闭过自启则不覆盖
- WiFi 列表周期刷新整体重建导致的闪烁：改为以 SSID 为键复用行元素，仅更新变化字段，进入动画只对新出现的行播放

## [0.2.6] - 2026-10-01

### 新增

- 状态动画增强为物理信号可视化：待命状态显示高斯三维分布线框曲面（呼吸起伏、缓慢旋转）；在线状态载波升级为 ASK 幅度调制，比特序列编码为高斯波包串并附比特刻度；恢复中显示高斯调制波包自左向右传播，成功锁定为连续载波，失败衰减为噪声

## [0.2.5] - 2026-10-01

### 变更

- 状态动画改版为贴近物理现实的形态：在线状态为发射点向上扩散的弧形辐射波（WiFi 信号的物理形态）；舞台底部新增连通性示波器，波形振幅映射连通性——在线满幅正弦波、离线塌缩为噪声抖动、尝试连接时振幅重建，恢复中波形为红色

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

[0.6.0]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.5.2...v0.6.0
[0.5.2]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.2.6...v0.3.0
[0.2.6]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/MisakaMikoto128/auto_wifi_connector/compare/bea81fa...v0.2.0

