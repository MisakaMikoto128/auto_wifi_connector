# 贡献指南

## 环境要求

- Windows 10/11（程序依赖 `netsh` 命令与 WebView2）
- Rust 1.77+（stable）

## 构建与运行

```sh
cd src-tauri
cargo run
```

前端是 `ui/` 目录下的静态文件，无构建步骤，修改后重启程序即可生效。

## 提交前检查

以下三项与 CI 一致，请在本地全部通过后再提交：

```sh
cd src-tauri
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

`cargo test --test live_reconnect -- --ignored` 是真实断网重连测试，会断开 WiFi 约 20 秒；改动 `wifi.rs` / `netcheck.rs` / `monitor.rs` 的连接与探测逻辑时应当运行。

## 提交信息

使用 Conventional Commits 格式：`类型: 简述`，例如：

- `feat: 候选网络支持手动置顶`
- `fix: 修复 TUN 模式下的在线误判`
- `docs: 更新 README 截图`

## 代码约定

- 注释与文档使用简洁准确的描述，不使用比喻、拟人等修辞
- 遵循最小修改原则：每次提交只解决一个问题
- 能不引入新依赖就不引入

## Pull Request 流程

1. Fork 仓库，从 `main` 创建分支
2. 提交修改并推送
3. 发起 PR，填写模板中的变更说明与检查清单
4. CI 通过后等待评审
