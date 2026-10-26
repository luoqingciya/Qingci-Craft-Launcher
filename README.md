<p align="center">
  <img src="assets/branding/app-icon.png" width="120" alt="Qingci Craft Launcher">
</p>

# Qingci Craft Launcher（清辞）

**中文** | [English](README.en.md)

[![CI](https://github.com/luoqingciya/Qingci-Craft-Launcher/actions/workflows/ci.yml/badge.svg)](https://github.com/luoqingciya/Qingci-Craft-Launcher/actions/workflows/ci.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)

用 **Tauri 2 + Rust** 构建的跨平台、轻量级 Minecraft Java 版启动器。

> 当前进度：**能装、能启动、能停止**。原版（26.3，以及 1.7.10 这类含 legacy 资产布局的旧版本）的完整链路已跑通：
> 拉清单 → 下载约 598 MB → 选 Java → 组装命令 → 启动 → 实时日志 → 优雅停止。
> 设置页可用（游戏目录 / 玩家名 / 内存 / Java）。Fabric 加载器的合并逻辑已完成，界面待接入。
> 路线图见 [docs/DEVELOPMENT_PLAN.md](docs/DEVELOPMENT_PLAN.md)，变更见 [CHANGELOG.md](CHANGELOG.md)。

## 功能现状

| 能力 | 状态 | 说明 |
|---|---|---|
| 版本清单 | ✅ | 从 Mojang 官方 `version.json` 推导，不硬编码版本列表 |
| 安装与校验 | ✅ | 版本文件、依赖库、资产、natives；逐个 SHA-1 校验 |
| Java 选择 | ✅ | 按版本要求自动挑选；也可手动指定，低于要求会直接拒绝 |
| 启动与日志 | ✅ | 实时转发 stdout/stderr，「停止」能真正终止进程 |
| 设置 | ✅ | 游戏目录、离线玩家名、最大内存、Java 运行时 |
| Fabric 加载器 | 🟡 | profile 解析与合并已完成并测试，**界面未接入** |
| 微软正版登录 | 🟡 | 设备码 + Xbox 认证链已完成并对着真实端点验证，**界面未接入**，需等 Mojang 审核 |
| 实例管理 | ⬜ | 多份独立游戏目录 |
| 模组 / Forge / 自动更新 | ⬜ | 见路线图 |

## 设计取向

- **轻量**：复用系统 WebView。便携版可执行文件 9.5 MB，NSIS 安装包 2.8 MB。
- **跨平台**：Windows 10+ / macOS 12+ / Linux（glibc 2.31+），三平台 CI 构建与测试。
- **可验证**：核心逻辑（版本解析、依赖求值、参数组装、认证协议）放在不依赖 GUI 的 Rust crate 中，CI 里 219 个 Rust 测试 + 27 个前端测试全量覆盖。
- **清单驱动**：版本、依赖库、JVM 参数全部从官方清单推导，不硬编码任何版本列表。

## 技术栈

| 层 | 选型 |
|---|---|
| 应用框架 | Tauri 2.12（Rust 1.99.1 / edition 2024，MSRV 1.90） |
| 前端 | React 19 + TypeScript 7 + Vite 8 + Tailwind CSS 4 |
| 异步与网络 | tokio + reqwest（rustls） |
| 前端质量 | vitest + Testing Library；oxlint（不用 eslint，见下） |
| 凭据存储 | 系统凭据库（规划中，见 [SECURITY.md](SECURITY.md)） |

> **为什么 lint 用 oxlint**：`typescript-eslint` 的 peer 依赖是 `typescript <6.1.0`，
> 而本项目用 TS 7（Go 原生编译器）。oxlint 自带 TS 解析、不依赖 `typescript` 包。

## 仓库结构

```
crates/          Rust 领域层（全部可在无 GUI 环境测试）
  qcl-core       版本解析、规则求值、启动命令组装（纯逻辑，无 IO）
  qcl-net        下载计划、执行器、HTTP 下载与 SHA-1 校验
  qcl-install    安装编排：版本文件、资产、natives、Fabric profile 合并
  qcl-java       Java 探测与按需安装
  qcl-runtime    游戏进程启动、日志转发与停止
  qcl-platform   本机平台探测（系统版本决定 GC 参数）
  qcl-auth       微软设备码登录与 Xbox 认证链（协议层，不落盘令牌）
  qcl-cli        无界面入口（冒烟与排查用）
src-tauri/       Tauri 应用层（薄适配，只做 IPC 与状态持有）
src/             React 19 前端
docs/            开发规划与架构决策记录
assets/          品牌资产（应用图标源文件，各平台图标由 tauri icon 生成）
```

## 安装

从 [Releases](https://github.com/luoqingciya/Qingci-Craft-Launcher/releases) 下载：

| 文件 | 说明 |
|---|---|
| `*-setup.exe` | NSIS 安装程序，装到用户目录，不需要管理员权限 |
| `*.msi` | MSI 安装包，适合批量部署 |
| 便携版 `.exe` | 免安装，游戏目录默认在同级 `game/` |

默认游戏目录是**安装目录下的 `game/`**（便携布局：整个目录拷走就能迁移）。
若安装位置不可写，会自动回退到应用数据目录并在界面提示原因。

## 本地开发

前置要求：

- Rust **1.99.1**（由 `rust-toolchain.toml` 固定，`rustup` 会自动安装）
- Node.js ≥ 22 与 pnpm ≥ 10
- Windows 需 MSVC Build Tools；Linux 需 `libwebkit2gtk-4.1-dev` 等依赖（见 CI 配置）

常用命令：

```bash
# Rust 侧
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-features

# 前端（lint + 单元测试 + 类型检查 + 生产构建）
pnpm install
pnpm lint
pnpm test
pnpm build

# 桌面应用（开发模式：热重载）
pnpm tauri dev

# 打包（产出安装程序）
pnpm tauri build              # 同时产出 NSIS 与 MSI
pnpm tauri build --bundles nsis

# 无界面入口：安装并打印启动命令，不启动游戏
cargo run -p qcl-cli -- 26.3 --dry-run
```

## 参与贡献

本项目严格采用 PR 流程：任何改动（含文档）都必须经分支 → PR → CI 全绿 → 评审通过 → Squash 合并。
详见 [CONTRIBUTING.md](CONTRIBUTING.md)（[English](CONTRIBUTING.en.md)）。

发版与在线更新的配置、密钥管理与排错见 [docs/RELEASE.md](docs/RELEASE.md)（[English](docs/RELEASE.en.md)）。

## 免责声明

本项目为非官方启动器，与 Mojang Studios、Microsoft 无任何隶属关系。
"Minecraft" 是 Mojang Studios 的商标。本项目不分发任何游戏本体、库文件或资源文件，全部内容在运行时从官方公开端点获取。
本项目不提供、也不会提供任何绕过正版验证的功能。

## 许可证

[GPL-3.0](LICENSE)，并附有 **GPLv3 第 7 条附加条款**（[ADDITIONAL-TERMS.md](ADDITIONAL-TERMS.md) · [English](ADDITIONAL-TERMS.en.md)）：

- 分发时须保留版权与许可声明
- **分发修改版须更换名称**，不得暗示与原版同源或获得原作者支持

原样分发未经修改的完整副本可以保留原名——附加条款不限制 GPLv3 允许的逐字分发。
