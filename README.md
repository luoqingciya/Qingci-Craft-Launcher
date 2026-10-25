<p align="center">
  <img src="assets/branding/app-icon.png" width="120" alt="Qingci Craft Launcher">
</p>

# Qingci Craft Launcher

[![CI](https://github.com/luoqingciya/Qingci-Craft-Launcher/actions/workflows/ci.yml/badge.svg)](https://github.com/luoqingciya/Qingci-Craft-Launcher/actions/workflows/ci.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)

用 **Tauri 2 + Rust** 构建的跨平台、轻量级 Minecraft Java 版启动器。

> 项目状态：**Phase 1（核心库）**。核心逻辑已成型（版本解析、规则求值、classpath 与参数组装、下载层、Java 探测），
> 尚未接入 UI。完整路线图见 [docs/DEVELOPMENT_PLAN.md](docs/DEVELOPMENT_PLAN.md)，变更记录见 [CHANGELOG.md](CHANGELOG.md)。

## 设计取向

- **轻量**：复用系统 WebView，安装包目标 ≤ 12 MB（Windows），空闲内存目标 ≤ 150 MB。
- **跨平台**：Windows 10+ / macOS 12+ / Linux（glibc 2.31+），三平台 CI 构建与测试。
- **可验证**：核心逻辑（版本解析、依赖求值、参数组装）放在不依赖 GUI 的 Rust crate 中，可在 CI 中完整覆盖。
- **清单驱动**：版本、依赖库、JVM 参数全部从 Mojang 官方 `version.json` 推导，不硬编码任何版本列表。

## 技术栈

| 层 | 选型 |
|---|---|
| 应用框架 | Tauri 2.12（Rust 1.90+ / edition 2024） |
| 前端 | React 19 + TypeScript + Vite + Tailwind CSS |
| 异步与网络 | tokio + reqwest（rustls） |
| 凭据存储 | 系统凭据库（Windows Credential Manager / macOS Keychain / Secret Service） |

## 仓库结构

```
crates/          Rust 领域层（qcl-core / qcl-net / qcl-java / qcl-auth / qcl-cli）
src-tauri/       Tauri 应用层（薄适配，不含业务算法）
src/             React 前端
docs/            开发规划与架构决策记录
assets/          品牌资产（应用图标源文件，各平台图标由 tauri icon 生成）
```

## 本地开发

前置要求：

- Rust **1.99.1**（由 `rust-toolchain.toml` 固定，`rustup` 会自动安装）
- Node.js ≥ 22 与 pnpm ≥ 10
- Windows 需 MSVC Build Tools；Linux 需 `libwebkit2gtk-4.1-dev` 等依赖（见 CI 配置）

常用命令：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## 参与贡献

本项目严格采用 PR 流程：任何改动（含文档）都必须经分支 → PR → CI 全绿 → 评审通过 → Squash 合并。
详见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 免责声明

本项目为非官方启动器，与 Mojang Studios、Microsoft 无任何隶属关系。
"Minecraft" 是 Mojang Studios 的商标。本项目不分发任何游戏本体、库文件或资源文件，全部内容在运行时从官方公开端点获取。
本项目不提供、也不会提供任何绕过正版验证的功能。

## 许可证

[GPL-3.0](LICENSE)
