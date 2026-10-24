# ADR-0001：采用 Tauri 2 而非 Electron

- 状态：已接受
- 日期：2026-10-24
- 决策者：@luoqingciya

## 背景

启动器需要跨 Windows / macOS / Linux 三个平台，并满足"轻量"这一核心目标。候选方案为 Electron、Tauri 2、以及纯原生 GUI（如 egui / iced）。

## 决策

采用 **Tauri 2.12.x**（Rust 1.90+，edition 2024），前端使用系统 WebView。

## 理由

1. **体积**：Electron 需要随包携带 Chromium 与 Node 运行时，仅空壳就超过 80 MB；Tauri 复用系统 WebView（Windows 用 WebView2），同等界面下安装包可控制在 10 MB 量级。
2. **内存**：Electron 空闲态通常在 200 MB 以上；Tauri 的 Rust 后端常驻内存为 10 MB 量级，主要开销来自 WebView 进程。
3. **语言一致性**：启动器的核心是文件下载、哈希校验、参数拼装、子进程管理，这些在 Rust 生态中实现质量更高，且与前端只通过 IPC 通信，边界清晰。
4. **纯原生 GUI 的排除理由**：启动器界面需要列表、滚动、富文本日志、图片展示，用 egui 这类即时模式 GUI 实现成本高且观感不佳。

## 后果

- 必须处理 WebView 平台差异：Windows 依赖 WebView2 运行时（旧系统需 bootstrapper）、macOS 依赖 WKWebView、Linux 依赖 WebKitGTK 且需在 CI 中安装系统依赖。
- Linux 打包需要 `libwebkit2gtk-4.1-dev` 等构建依赖，CI 配置复杂度上升。
- 前端与后端跨语言，需要维护 IPC 契约（见 `docs/DEVELOPMENT_PLAN.md` §3.4）。

## 替代方案

- **Electron**：生态最成熟、调试最简单，但违背体积与内存目标。
- **Wails（Go）**：与 Tauri 定位接近，但 Rust 在哈希校验、并发下载、跨平台进程管理上更合适，且团队技术栈偏向 Rust。
