<p align="center">
  <img src="assets/branding/app-icon.png" width="120" alt="Qingci Craft Launcher">
</p>

# Qingci Craft Launcher（清辞）

**中文** | [English](README.en.md)

[![CI](https://github.com/luoqingciya/Qingci-Craft-Launcher/actions/workflows/ci-main.yml/badge.svg)](https://github.com/luoqingciya/Qingci-Craft-Launcher/actions/workflows/ci-main.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)

用 **Tauri 2 + Rust** 构建的跨平台、轻量级 Minecraft Java 版启动器。

> 当前进度：**能装、能启、能管模组、能自动更新**。
> 原版（26.3，以及 1.7.10 这类含 legacy 资产布局的旧版本）的完整链路已跑通：
> 拉清单 → 下载约 598 MB → 选 Java → 组装命令 → 启动 → 实时日志 → 优雅停止。
> Fabric 加载器可选；模组页支持搜索、安装（自动补必需依赖）、卸载与更新检查，
> 已装的项目直接标出，装前能看到会带上哪些依赖，检查更新时同时报出不适配当前版本的模组。
> 日志区可按级别筛选、搜索、复制。
> 异常退出会给崩溃摘要，并读取游戏自己写的 `crash-reports/`。
> 安装版支持在线更新：查到新版本后下载、校验签名、静默安装并重启。
> 路线图见 [docs/DEVELOPMENT_PLAN.md](docs/DEVELOPMENT_PLAN.md)，变更见 [CHANGELOG.md](CHANGELOG.md)。

## 功能现状

| 能力 | 状态 | 说明 |
|---|---|---|
| 版本清单 | ✅ | 从 Mojang 官方 `version.json` 推导，不硬编码版本列表 |
| 安装与校验 | ✅ | 版本文件、依赖库、资产、natives；逐个 SHA-1 校验 |
| Java 选择 | ✅ | 按版本要求自动挑选；也可手动指定，低于要求会直接拒绝 |
| 启动与日志 | ✅ | 实时转发 stdout/stderr，「停止」能真正终止进程 |
| 设置 | ✅ | 游戏目录、离线玩家名、最大内存、Java 运行时、下载源 |
| Fabric 加载器 | ✅ | 选版本 → 选加载器 → 安装 → 启动；合并结果落盘，重启后仍可用 |
| 模组管理 | ✅ | Modrinth 搜索 / 安装 / 卸载 / 更新检查；安装时自动补必需依赖，装前可预览依赖 |
| 模组状态 | ✅ | 已装的项目直接标出；检查更新时同时报出不适配当前版本的模组 |
| 中文译名与筛选 | ✅ | 译名库远端托管、启动时拉取并缓存；中文关键词别名 + 19 个分类中文化 |
| 日志工具 | ✅ | 按级别筛选、关键词搜索、一键复制（复制的是筛选后的行） |
| 崩溃摘要 | ✅ | 非 0 退出时给出结论，并读取 `crash-reports/` 里的报告 |
| 在线更新 | ✅ | 安装版自动检查，签名校验通过才安装 |
| 微软正版登录 | ✅ | 设备码流程（启动器碰不到密码）；令牌存系统凭据库，启动时静默刷新 |
| 实例管理 | ✅ | 多份独立游戏目录，各自的版本选择互不干扰；启动页可直接打开目录、看到已装模组数 |
| 运行环境 | ✅ | 关于页显示版本 / 系统架构 / Tauri / WebView，一键复制成反馈文本 |
| Forge / NeoForge | ⬜ | 见路线图 |

## 设计取向

- **轻量**：复用系统 WebView。便携版可执行文件 12.7 MB，NSIS 安装包 3.8 MB。
- **跨平台**：Windows 10+ / macOS 12+ / Linux（glibc 2.31+），三平台 CI 构建与测试。发布产物覆盖 Windows、macOS 与 Linux。
- **可验证**：核心逻辑（版本解析、依赖求值、参数组装、认证协议、模组解析、崩溃报告解析）放在不依赖 GUI 的 Rust crate 中，CI 里 340 个 Rust 测试 + 183 个前端测试全量覆盖。
- **清单驱动**：版本、依赖库、JVM 参数全部从官方清单推导，不硬编码任何版本列表。

## 技术栈

| 层 | 选型 |
|---|---|
| 应用框架 | Tauri 2.12（Rust 1.99.1 / edition 2024，MSRV 1.90） |
| 前端 | React 19 + TypeScript 7 + Vite 8 + Tailwind CSS 4 |
| 异步与网络 | tokio + reqwest（rustls） |
| 前端质量 | vitest + Testing Library；oxlint（不用 eslint，见下） |
| 更新分发 | `tauri-plugin-updater` + 腾讯云 COS（签名校验，见 [docs/RELEASE.md](docs/RELEASE.md)） |
| 凭据存储 | 系统凭据库：Windows Credential Manager / macOS Keychain / Secret Service（见 [SECURITY.md](SECURITY.md)） |

> **为什么 lint 用 oxlint**：`typescript-eslint` 的 peer 依赖是 `typescript <6.1.0`，
> 而本项目用 TS 7（Go 原生编译器）。oxlint 自带 TS 解析、不依赖 `typescript` 包。

## 仓库结构

```
crates/          Rust 领域层（全部可在无 GUI 环境测试）
  qcl-core       版本解析、规则求值、启动命令组装（纯逻辑，无 IO）
  qcl-net        下载计划、执行器、HTTP 下载与 SHA-1 校验
  qcl-install    安装编排：版本文件、资产、natives、Fabric profile 合并
  qcl-java       Java 探测与按需安装
  qcl-runtime    游戏进程启动、日志转发、停止，以及 crash-reports 解析
  qcl-platform   本机平台探测（系统版本决定 GC 参数）
  qcl-auth       微软设备码登录与 Xbox 认证链（协议层，不落盘令牌）
  qcl-mods       Modrinth 搜索/安装/卸载/更新检查、依赖补全、中文译名库
  qcl-cli        无界面入口（冒烟与排查用）
src-tauri/       Tauri 应用层（薄适配，只做 IPC 与状态持有）
src/             React 19 前端
docs/            开发规划、发版说明与架构决策记录
assets/          品牌资产（应用图标源文件、模组译名库数据）
```

## 安装

从 [Releases](https://github.com/luoqingciya/Qingci-Craft-Launcher/releases) 下载：

| 平台 | 文件 | 大小 | 说明 |
|---|---|---|---|
| Windows | `QingciCraftLauncher_*_x64-setup.exe` | 3.8 MB | NSIS 安装程序，装到用户目录，不需要管理员权限；支持自动更新 |
| Windows | `QingciCraftLauncher_*_x64-portable.exe` | 12.7 MB | 免安装，双击即用；游戏目录在 exe 旁边；**不支持自动更新** |
| macOS | `QingciCraftLauncher_*_universal.dmg` | 12.1 MB | 通用二进制，Intel 与 Apple Silicon 都能跑；**未签名**，首次打开需要右键「打开」 |
| Linux | `QingciCraftLauncher_*_amd64.deb` | 5.3 MB | Debian / Ubuntu 安装包，依赖系统已有的 webkit2gtk |
| Linux | `QingciCraftLauncher_*_x86_64.AppImage` | 83.5 MB | 免安装，自包含运行时 |

macOS、Linux 产物与 Windows 便携版**只在 GitHub Releases 提供**，不走自动更新
（更新服务器上只放 Windows 安装包）。

### macOS 首次打开

dmg 里的应用**没有签名，也没有经过公证**——这需要付费的 Apple 开发者账号。
macOS 会因此拒绝直接双击运行，提示「无法验证开发者」。三种打开方式任选：

1. **右键点应用 → 打开**，在弹出的对话框里再点「打开」（只需一次）
2. 系统设置 → 隐私与安全性 → 底部会有一条拦截记录 → 「仍要打开」
3. 终端执行 `xattr -dr com.apple.quarantine "/Applications/Qingci Craft Launcher.app"`

第 3 种最省事，也最不直观——**没有别的办法**，除非买开发者账号做签名与公证。
这是未签名分发的固有代价，不是这一版的问题。

安装位置没有限制，但**不要放在只读位置**：游戏目录默认跟着应用走，不可写时会回退到
`~/Library/Application Support/cn.luoqingciya.qcl/`（界面会提示原因）。

三个平台体积差这么多，是因为**复用系统 WebView 的程度不同**：Windows 用系统自带的 WebView2，
deb 依赖发行版仓库里的 `libwebkit2gtk-4.1`，而 AppImage 必须自带一整套 WebKitGTK +
JavaScriptCore + GTK3 才能在任意发行版上直接跑——那套东西本身就占 70 MB 以上。
只要体积就选 deb，要「下载就能双击运行」就选 AppImage。

体积取当前版本的实测值，版本之间会有小幅浮动。

**下载源可切换**：设置里能选 Mojang 官方（默认）或 [BMCLAPI](https://bmclapidoc.bangbang93.com/)
镜像。镜像失败、或下到的文件校验不符，都会**自动回落到官方**——它只是「更快的一条路」，
不是唯一依赖。走镜像时界面会标注来源，这也是 BMCLAPI 使用条款的要求。

默认游戏目录是**安装目录下的 `game/`**（便携布局：整个目录拷走就能迁移）。
若安装位置不可写，会自动回退到应用数据目录并在界面提示原因。

**可以有多份游戏目录（实例）**：设置页能新建、重命名、移除实例，每个实例有自己的目录和
自己的版本选择——原版玩一个、整合包玩另一个，切换时互不干扰。实例多于一个时，启动面板
会出现切换器。新建实例默认落在 `instances/<名字>`，也可以指到任意位置。
**移除只从列表里去掉，不会删磁盘上的文件**。

**安装版与便携版的设置互不影响**：安装版存在 `%APPDATA%\cn.luoqingciya.qcl\`，
便携版存在 exe 旁边（`settings.json`）。两者用的是同一个应用标识符，早期共用一份配置，
结果一边选的版本会出现在另一边——现在分开了。

**同一时间只开一个启动器窗口**：再双击会聚焦已有窗口，而不是开出第二个。注意安装版与便携版
共用应用标识符，所以两者之间也是互斥的——已经开着安装版时再双击便携版，会跳到安装版的窗口。

### 在线更新

**仅 Windows 安装版支持**。启动后会自动检查更新（查不到就什么也不提示）。查到新版本时
「关于」页会给出提示与更新说明，点「下载并安装」后：

1. 从 COS 下载安装包
2. **用内置公钥校验 minisign 签名**，对不上就拒绝安装
3. 静默安装并自动重启

不支持的两种情况，界面都会直接引导到下载页，而不是给一个点了就报错的按钮：

- **便携版**：没有卸载器，Windows 不允许替换正在运行的 exe
- **Linux**：安装包（AppImage / deb）不上 COS，只在 GitHub Releases 提供

配置与密钥管理见 [docs/RELEASE.md](docs/RELEASE.md)。

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
