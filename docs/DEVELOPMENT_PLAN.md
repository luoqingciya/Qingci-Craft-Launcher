# Qingci Craft Launcher 开发规划

> 文档状态：Draft v0.1 ｜ 最后更新：2026-10-24 ｜ 仓库：`luoqingciya/Qingci-Craft-Launcher`
> 本文档为项目立项基线。所有技术决策以本文为准；变更须走 PR 并同步更新本文件。

---

## 1. 项目定位

**一句话定义**：用 Tauri 2 + Rust 构建的跨平台、轻量级 Minecraft Java 版启动器。

### 1.1 设计目标（可度量，非口号）

| 维度 | 目标值 | 度量方式 |
|---|---|---|
| 安装包体积 | Windows NSIS ≤ 12 MB；macOS dmg ≤ 18 MB；Linux AppImage ≤ 25 MB | CI 产物大小断言 |
| 冷启动到可交互 | ≤ 1.5 s（SSD，二次启动） | 前端 `performance.now()` 埋点，CI 记录趋势 |
| 空闲内存占用 | ≤ 150 MB（含 WebView2 进程） | 手工基准，Release 前记录 |
| 支持平台 | Windows 10+ / macOS 12+ / Linux (glibc 2.31+) | CI 构建矩阵 + 手工冒烟 |
| 首次全量下载 | 与官方启动器持平（并发 16，实测受网络限制） | 日志统计耗时 |

### 1.2 非目标（明确排除，避免范围蔓延）

1. **不实现任何绕过正版验证的功能**。离线模式仅用于无网络环境与本地开发，UI 上必须显式标注"离线模式"。
2. **不分发 Minecraft 客户端本体、库文件、资源文件**。全部在运行时从官方端点下载。
3. 不自建 Mod 分发站、不运营整合包仓库（仅消费 Modrinth / CurseForge 公开 API）。
4. 不做游戏内注入（Mod 运行是 loader 的职责，启动器只负责拼装 classpath 与参数）。
5. 不做游戏内叠加层（Overlay）、不修改客户端 jar。

### 1.3 合规基线

- 遵循 Minecraft EULA 与 Mojang 关于第三方启动器的公开指引。
- 只调用官方公开端点：`piston-meta.mojang.com`、`resources.download.minecraft.net`、`api.minecraftservices.com`、`launchermeta.mojang.com`。
- 项目沿用已有 `LICENSE`（GPL-3.0）。依赖许可证由 `cargo-deny` 白名单管控；Tauri 生态（Apache-2.0 / MIT）与 GPLv3 兼容，无需特殊处理。

### 1.4 已确认决策（2026-10-24）

| # | 待决事项 | 结论 | 落地影响 |
|---|---|---|---|
| D1 | 是否支持 Forge / NeoForge | **支持** | Phase 4 保留对应 PR，拆为独立 PR 置于末尾；Fabric / Quilt 先行交付 |
| D2 | 前端框架 | **React 19** | 取代原 Svelte 5 方案；首屏 gzip 预算上调至 150 KB，换取生态与可维护性 |
| D3 | 是否先验证三平台打包链路 | **是** | Phase 0 先产出 `v0.0.1-alpha` 三平台安装包，再进入业务开发 |

---

## 2. 技术选型与依据

### 2.1 选型表

| 层次 | 选型 | 依据 |
|---|---|---|
| 应用框架 | **Tauri 2.12.x** | 复用系统 WebView（Windows 用 WebView2），Rust 后端常驻内存仅约 10 MB 量级，包体积远小于 Electron |
| 前端 | **React 19 + TypeScript + Vite + Tailwind CSS v4** | 生态成熟、类型定义完善、可维护性优先；配合路由级代码分割把首屏 gzip 控制在 150 KB 内（相对 12 MB 安装包影响 < 1.5%），体积代价可接受 |
| 异步运行时 | `tokio`（`rt-multi-thread`, `fs`, `process`, `sync`） | 下载并发与子进程管理是核心路径 |
| HTTP | `reqwest`（`rustls-tls`, `http2`, `gzip`） | 用 rustls 规避 OpenSSL，三平台交叉编译无需额外系统依赖 |
| 序列化 | `serde` + `serde_json` | Mojang 全部接口为 JSON |
| 校验 | `sha1`（RustCrypto） | Mojang 资源校验算法为 SHA-1 |
| 错误处理 | `thiserror`（库）/ `anyhow`（应用层） | 库层需要可分类错误，应用层只需上下文链 |
| 日志 | `tracing` + `tracing-subscriber` | 结构化 span，可同时落文件与推送到前端控制台 |
| 令牌存储 | `keyring`（v3） | 走系统凭据库（Windows Credential Manager / macOS Keychain / Linux Secret Service），**不落明文** |
| 配置存储 | JSON 文件（`serde`） | 实例量级为个位数到几十，引入 SQLite 属于过度设计 |
| 单实例 / 自动更新 | `tauri-plugin-single-instance` / `tauri-plugin-updater` | 官方插件，签名校验内置 |
| 前端测试 | `vitest` + `@testing-library/react` | 与 Vite 同源，零额外构建配置 |
| E2E | WebdriverIO + `tauri-driver` | 官方推荐路径；仅 Windows/Linux 纳入 CI（macOS 需要额外辅助功能授权） |

### 2.2 上游实测数据（2026-10-24 采集，作为设计输入）

| 项目 | 实测值 | 对设计的影响 |
|---|---|---|
| `tauri` crate 最新稳定版 | **2.12.5**（2026-10-21 发布） | 锁 `2.12`，不使用 `3.0.0-alpha.5` |
| `tauri` 2.12.5 的 MSRV / edition | **Rust 1.90** / **edition 2024** | workspace 的 `rust-version` 与 `edition` 以此为准 |
| 官方最新正式版 | **26.3**（2026-09-15）；快照 `26.4-snapshot-5` | 版本号已是 `26.x` 新方案，**不得硬编码版本列表**，必须完全由 manifest 驱动 |
| 26.3 的 `javaVersion` | `majorVersion: 25`，`component: java-runtime-epsilon` | 需要按版本动态选择 JRE（8 / 17 / 21 / 25） |
| 26.3 依赖规模 | 114 个 libraries，client jar 39.6 MB，assetIndex `34` 合计 462 MB | 首次启动下载量约 500 MB，下载子系统是性能关键路径 |
| Temurin JRE 体积 | 25 → 55.8 MB；21 → 46.7 MB；17 → 41.8 MB；8 → 38.2 MB | JRE 必须按需下载、按版本隔离，不能捆绑进安装包 |
| Fabric loader 最新稳定版 | 0.20.1 | loader 支持走 meta API 即可，无需本地安装器 |
| 版本清单条目总数 | 920 | 全量清单约 1 MB 级，需本地缓存 + 条件请求 |

**结论**：Java 版本、库列表、参数模板三者都必须从 `version.json` 动态推导，任何硬编码都会在下一个版本失效。

---

## 3. 系统架构

### 3.1 分层

```
┌──────────────────────────────────────────────────────────────┐
│  React 19 + Vite（系统 WebView）                             │
│  Views: 版本列表 / 实例管理 / 账号 / 设置 / 控制台            │
└──────────────▲───────────────────────────┬───────────────────┘
               │ invoke(cmd)               │ emit(event)
┌──────────────┴───────────────────────────▼───────────────────┐
│  src-tauri：应用层                                           │
│  command handlers · AppState · 事件桥 · 托盘 · 单实例         │
│  职责边界：参数校验、状态编排、事件转发。不含业务算法。       │
└──────────────▲───────────────────────────┬───────────────────┘
               │                           │
┌──────────────┴───────────────────────────▼───────────────────┐
│  Rust 领域层（可脱离 UI 独立测试、可被 CLI 复用）             │
│  qcl-core  版本解析 · rules 求值 · 参数组装 · 实例 · 启动编排 │
│  qcl-net   并发下载 · SHA-1 校验 · 重试退避 · 进度上报        │
│  qcl-auth  微软 OAuth 设备码 · XBL/XSTS · 令牌生命周期        │
│  qcl-java  JRE 探测 · Mojang runtime 安装 · 版本匹配          │
└──────────────▲───────────────────────────┬───────────────────┘
               │                           │
        piston-meta · resources.download · api.minecraftservices · adoptium
```

**关键约束**：`src-tauri` 只做薄适配层。所有可测逻辑下沉到 `crates/`，保证在 CI 无 GUI 环境下可完整覆盖。

### 3.2 Crate 划分

| Crate | 职责 | 关键依赖 | 测试策略 |
|---|---|---|---|
| `qcl-core` | 领域模型、manifest 解析、rules 求值、classpath 组装、参数模板替换、实例管理 | serde, thiserror | 单测 + `insta` 快照（以 26.3 真实 json 为夹具） |
| `qcl-net` | 下载队列、并发控制、校验、重试、进度事件、磁盘预检 | reqwest, tokio, sha1 | `wiremock` 模拟端点 + 失败注入 |
| `qcl-auth` | 设备码登录、XBL/XSTS 换取、刷新、`keyring` 持久化 | reqwest, keyring | 纯函数化状态机单测 + 录制回放 |
| `qcl-java` | 本机 JRE 探测（JAVA_HOME / PATH / 常见安装路径 / 注册表）、按需安装 | reqwest, zip | 目录扫描夹具单测 |
| `qcl-cli` | headless 启动入口，用于 CI 冒烟与问题复现 | clap | 集成测试（Linux + xvfb） |
| `src-tauri` | IPC 命令、事件、窗口、托盘 | tauri, 上述 crate | E2E（可选） |

### 3.3 一次启动的数据流

1. 读取本地 `version_manifest` 缓存 → 条件请求（`If-None-Match`）校验更新。
2. 取目标版本 `version.json`（缓存至 `versions/<id>/<id>.json`）。
3. 解析 `libraries`：逐条求值 `rules`（os.name / os.arch / os.version / features）→ 过滤出当前平台所需库。
4. 解压 `classifiers` 中的 natives 到 `versions/<id>/natives/`（排除 `META-INF/`）。
5. 校验 `assets`：按 `assetIndex` 拉取索引 → 并发下载缺失对象到 `assets/objects/<前2位>/<hash>`。
6. 选择 JRE：读 `javaVersion.majorVersion` → 匹配 `runtime/` 下已安装运行时；缺失则安装。
7. 组装命令行：`arguments.jvm` + `arguments.game`，做变量替换（`${classpath}`、`${auth_player_name}`、`${assets_root}`、`${natives_directory}` 等），并求值 `rules` 中的 `features`。
8. 以参数数组形式启动子进程（**禁止字符串拼接**），Windows 使用 `javaw.exe` 避免控制台窗口。
9. 转发 stdout/stderr → `tracing` + 前端控制台事件；解析退出码。
10. 退出后写入本次会话记录（耗时、退出码、崩溃标记）。

### 3.4 IPC 契约（首版）

**命令**：`list_versions`、`get_version_detail`、`create_instance`、`update_instance`、`delete_instance`、`install_version`、`launch_instance`、`kill_instance`、`login_device_code`、`poll_login`、`logout`、`list_accounts`、`get_settings`、`save_settings`、`open_game_dir`

**事件**：`download://progress`（`{task_id, phase, current, total, bytes_per_sec}`）、`launch://state`（`installing | launching | running | exited`）、`log://line`（`{stream, line}`）

约定：所有命令返回 `Result<T, AppError>`，`AppError` 序列化为 `{code, message, detail}`，前端按 `code` 做本地化，不解析英文消息。

---

## 4. 核心模块设计

### 4.1 版本解析与依赖求值

- `rules` 求值顺序：默认 **deny**，按数组顺序匹配，命中 `allow` 则允许，命中 `deny` 则拒绝；`rules` 为空视为允许。
- `features` 支持集合：`is_demo_user`、`has_custom_resolution`、`has_quick_plays_support`、`is_quick_play_singleplayer`、`is_quick_play_multiplayer`、`is_quick_play_realms`。
- **1.13 之前**的版本使用 `minecraftArguments` 字符串（空格分隔）而非 `arguments` 数组，需双路径支持。
- `os.version` 用正则匹配（如 `^10\.`），仅在声明时求值。
- classpath 去重按 `group:artifact` 保留版本号最高者（loader 场景常见冲突），顺序保持 manifest 顺序（顺序影响类加载）。

### 4.2 下载子系统

| 关注点 | 方案 |
|---|---|
| 并发 | `tokio::sync::Semaphore`，默认 16，可在设置中调 1–32 |
| 校验 | 下载完成后比对 SHA-1；已存在文件先校验再跳过（避免半文件） |
| 重试 | 指数退避，最多 3 次，仅对 5xx / 超时 / 连接错误重试 |
| 断点 | 大文件（client jar、JRE）用 `Range` 续传，小文件直接重下 |
| 进度 | 聚合字节级进度，200 ms 节流上报，避免 IPC 洪泛 |
| 磁盘预检 | 启动前校验剩余空间 ≥ 预估下载量 × 1.2 |
| 原子性 | 先写 `.part` 临时文件，校验通过后 `rename` |
| 镜像 | 预留 `download_source` 抽象（官方 / BMCLAPI），首版只实现官方 |

### 4.3 Java 运行时管理

**探测优先级**（用于"使用本机 Java"）：

1. 实例级显式指定的 `java_path`
2. 设置中的全局 `java_path`
3. `JAVA_HOME`
4. `PATH` 中的 `java`
5. 平台常见路径（Windows 注册表 `JDK`/`JRE` 键 + `Program Files\Eclipse Adoptium\*`；macOS `/Library/Java/JavaVirtualMachines/*`；Linux `/usr/lib/jvm/*`）
6. 启动器自管目录 `runtime/<component>/`

**按需安装**：优先使用 Mojang 官方 runtime 清单（`launchermeta.mojang.com/v1/products/java-runtime/.../all.json`），其中按 `component` + 平台给出文件级 SHA-1，可精确复现官方运行时；Mojang 未覆盖的平台/架构回退到 Adoptium API。

**版本匹配规则**：`majorVersion` 精确匹配优先；找不到则向上取最近的更高版本（如 26.3 需 25，本机有 25 则用，只有 21 则提示安装）。**不允许**向下兼容猜测。

### 4.4 认证（微软账号）

采用 **OAuth 2.0 设备码流程**（无需 client secret、无需本地回调端口，最适合桌面应用）：

```
POST https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode
     scope = "XboxLive.signin offline_access"
  → 展示 user_code + verification_uri 给用户

POST .../oauth2/v2.0/token
     grant_type = urn:ietf:params:oauth:grant-type:device_code

POST https://user.auth.xboxlive.com/user/authenticate        (RpsTicket = "d=<ms_access_token>")
POST https://xsts.auth.xboxlive.com/xsts/authorize           (RelyingParty = "rp://api.minecraftservices.com/")
POST https://api.minecraftservices.com/authentication/login_with_xbox
     identityToken = "XBL3.0 x=<uhs>;<xsts_token>"
GET  https://api.minecraftservices.com/minecraft/profile      (Bearer)
```

必须处理的 XSTS 错误码：`2148916233`（无 Xbox 账户）、`2148916235`（地区受限）、`2148916238`（未成年需加入家庭组）。这三个错误在中文用户群中出现频率不低，需要专门的引导文案而非泛化报错。

**令牌策略**：Minecraft `access_token` 有效期 24 h；`refresh_token` 存入 `keyring`，启动时静默刷新；刷新失败才回落到登录界面。任何令牌都不写入日志与配置文件。

### 4.5 启动与进程管理

- 使用 `tokio::process::Command` 以**参数数组**传参，杜绝空格/中文路径导致的引号问题。
- Windows 下指向 `javaw.exe`（无控制台）；stdout/stderr 仍重定向以捕获日志。
- 记录 `Child` 句柄，提供 `kill_instance`；应用退出时询问是否终止游戏进程。
- 退出码映射：`0` 正常；非 0 附最后 50 行 stderr 生成崩溃摘要，引导用户导出日志包。
- 启动器自身崩溃不影响已启动的游戏进程（子进程独立）。

### 4.6 存储布局

沿用官方 `.minecraft` 的目录语义，便于用户迁移与"导入现有游戏目录"：

```
<AppData>/QingciCraftLauncher/
├─ versions/<id>/<id>.json, <id>.jar, natives/
├─ libraries/<group path>/
├─ assets/{indexes,objects,virtual}/
├─ runtime/<component>/<platform>/
├─ instances/<instance-id>/instance.json
├─ logs/
└─ config.json
```

---

## 5. 数据模型（草案）

```rust
pub struct Instance {
    pub id: String,              // uuid v4
    pub name: String,
    pub game_version: String,    // "26.3"
    pub loader: Loader,          // Vanilla | Fabric { version } | Quilt { version } | Forge { version } | NeoForge { version }
    pub java: JavaPreference,    // Auto | Path(PathBuf)
    pub memory_mb: u32,          // 默认 4096
    pub jvm_args: Vec<String>,   // 追加在 manifest 之后
    pub game_args: Vec<String>,
    pub resolution: Option<(u32, u32)>,
    pub created_at: i64,
    pub last_played_at: Option<i64>,
}

pub struct Account {
    pub id: String,              // minecraft uuid
    pub name: String,
    pub kind: AccountKind,       // Microsoft | Offline
    pub skin_model: SkinModel,   // Classic | Slim
}
// 令牌不在此结构中，按 account_id 存 keyring

pub enum LaunchEvent {
    Resolving, Downloading { done: u64, total: u64 }, Installing { step: String },
    Starting, Running { pid: u32 }, Exited { code: Option<i32> }, Failed { code: String },
}
```

离线 UUID 按官方算法生成：`MD5("OfflinePlayer:" + name)` 并置版本位为 3（与官方启动器一致，保证存档与服务器识别一致）。

---

## 6. 目录结构

```
Qingci-Craft-Launcher/
├─ .github/
│  ├─ workflows/{ci.yml, release.yml}
│  ├─ ISSUE_TEMPLATE/{bug.yml, feature.yml}
│  ├─ PULL_REQUEST_TEMPLATE.md
│  ├─ CODEOWNERS
│  └─ dependabot.yml
├─ crates/
│  ├─ qcl-core/  qcl-net/  qcl-auth/  qcl-java/  qcl-cli/
├─ src-tauri/          # Tauri 应用（Cargo 包名 qcl-app）
├─ src/                # React 前端
├─ docs/
│  ├─ DEVELOPMENT_PLAN.md   # 本文件
│  └─ adr/                  # 架构决策记录（每个重大选型一篇）
├─ Cargo.toml          # workspace
├─ rust-toolchain.toml # 固定工具链，CI 与本机一致
├─ clippy.toml / rustfmt.toml / deny.toml
├─ package.json / vite.config.ts / tsconfig.json / eslint.config.js
├─ CONTRIBUTING.md  SECURITY.md  CHANGELOG.md
└─ LICENSE (GPL-3.0)
```

---

## 7. 里程碑与 PR 拆分

原则：**每个 PR 单一职责、可独立回滚、含测试、CI 全绿后由维护者合并**。下表每个 PR 即一个独立分支。

### Phase 0 · 工程基建（先立规矩，再写代码）

| PR | 内容 | 验收标准 |
|---|---|---|
| #1 | `chore: 仓库基建 + workspace 骨架` — README、CONTRIBUTING、SECURITY、PR/Issue 模板、CODEOWNERS、`.gitignore`、`rust-toolchain.toml`、rustfmt/clippy/deny 配置、dependabot，以及根 `Cargo.toml` 与 `qcl-core` 空 crate | 模板生效；`cargo fmt --check` 与 `cargo test` 在本地和 CI 均可执行 |
| #2 | `ci: PR 质量门禁` — fmt / clippy / test 三平台矩阵 / 前端检查 / cargo-deny | 三平台 job 全绿，耗时 < 12 min |
| #3 | `ci: 发布流水线 + Tauri 最小骨架` — hello-world 窗口 + tag 触发三平台打包 | 打 `v0.0.1-alpha` 能产出三平台未签名安装包（draft release） |

> Phase 0 的 PR #3 同时验证"打包链路是否真的三平台可用"。这一步不放后面做，因为签名、WebView 依赖、交叉编译问题一旦在 Phase 4 才暴露，返工成本最高。
>
> PR #1 之所以包含 workspace 骨架：没有可编译的代码，CI 门禁就是空转。骨架只放最小可编译单元，领域模型仍按 PR #4 起逐步落地。

### Phase 1 · 核心库（无 UI，纯 Rust，CI 全覆盖）

| PR | 内容 | 验收标准 |
|---|---|---|
| #4 | `feat(core): 领域模型 + manifest 拉取与缓存` | 单测覆盖缓存命中/失效；`insta` 快照固定 26.3 解析结果 |
| #5 | `feat(core): rules 求值 + 库解析 + classpath 组装` | 对 26.3 真实 json 断言库数量与顺序；覆盖 pre-1.13 `minecraftArguments` 分支 |
| #6 | `feat(net): 并发下载器` — 校验/重试/进度/断点 | `wiremock` 注入 500、超时、哈希不匹配三种故障，断言重试与失败语义 |
| #7 | `feat(core): 资产索引 + natives 解压 + 参数模板替换` | 断言 `virtual` 资产映射与 `${}` 变量全部替换完毕（无残留 `${`） |
| #8 | `feat(java): JRE 探测 + Mojang runtime 安装` | 目录夹具覆盖 Win/mac/Linux 布局；版本匹配规则单测 |
| #9 | `feat(cli): headless 启动闭环` | CI 上 `qcl-cli launch 26.3` 下载完成并启动至主菜单（Linux + xvfb），退出码 0 |

### Phase 2 · 桌面 MVP

| PR | 内容 | 验收标准 |
|---|---|---|
| #10 | `feat(app): IPC 命令层 + 事件桥` | 前端可收到节流后的进度事件；命令错误结构统一 |
| #11 | `feat(ui): 版本列表 + 下载进度 + 启动按钮` | 三平台手工冒烟：全新环境装包 → 启动 26.3 到主菜单 |
| #12 | `feat(ui): 实例管理 + 设置（内存/JVM 参数/Java 路径）` | 配置持久化并在重启后生效 |
| #13 | `feat(app): 日志控制台 + 崩溃摘要` | 非 0 退出码能生成可导出的日志包 |

### Phase 3 · 账号

| PR | 内容 | 验收标准 |
|---|---|---|
| #14 | `feat(auth): 设备码登录 + 令牌刷新 + keyring 存储` | 断网/过期场景下静默刷新成功；令牌不出现于任何日志 |
| #15 | `feat(ui): 登录流程 + 皮肤头像` | 正版账号启动后显示正确用户名与皮肤头部 |

### Phase 4 · 模组加载器

| PR | 内容 | 验收标准 |
|---|---|---|
| #16 | `feat(loader): Fabric / Quilt`（meta profile 合并到 version.json） | 启动 Fabric 0.20.1 + 26.3 成功 |
| #17 | `feat(loader): Forge / NeoForge`（安装器流程 + 库合并） | 至少覆盖一个 LTS 版本；失败时有明确错误码 |
| #18 | `feat(mods): Modrinth 搜索与安装` | 依赖解析不破坏现有实例（安装前快照） |

### Phase 5 · 发布工程

签名（Windows Authenticode / macOS codesign + notarize）、`tauri-plugin-updater` 自动更新、i18n（zh-CN / en-US）、`v1.0.0` 正式版。

**版本号策略**：MVP 前保持 `0.x.y`，Phase 5 完成后发布 `1.0.0`；此后 SemVer + `CHANGELOG.md` 由 `git-cliff` 从 Conventional Commits 生成。

---

## 8. CI/CD 设计

### 8.1 PR 门禁（`.github/workflows/ci.yml`）

触发：`pull_request`（含 draft 转 ready）与 `push` 到 `main`。

| Job | 运行环境 | 内容 |
|---|---|---|
| `fmt` | ubuntu-latest | `cargo fmt --all --check`（最快失败，先跑） |
| `clippy` | ubuntu-latest | `cargo clippy --workspace --all-targets -- -D warnings` |
| `test` | **matrix: ubuntu / windows / macos** | `cargo test --workspace --all-features`；ubuntu 额外跑 `cargo llvm-cov` 并上传 Codecov |
| `frontend` | ubuntu-latest | `pnpm install --frozen-lockfile` → `tsc --noEmit` → `eslint` → `vitest run` → `vite build` |
| `deny` | ubuntu-latest | `cargo deny check`（许可证白名单 + 已知漏洞 + 重复依赖） |
| `build` | **matrix: 三平台** | `pnpm tauri build --no-bundle`（PR 只编译不打包，约 6–8 min） |

**缓存**：`Swatinem/rust-cache@v2`（按 `Cargo.lock` 哈希 + target 分键）、pnpm store。实测可把三平台总耗时压到 10 min 内。

**Linux 构建前置依赖**（易漏，必须写进 workflow）：`libwebkit2gtk-4.1-dev`、`libappindicator3-dev`、`librsvg2-dev`、`patchelf`、`libssl-dev`。

**`build` job 的存在理由**：本机通过、CI 失败是跨平台项目最常见的坑（Windows 路径大小写、macOS SDK 差异、Linux WebKit 版本）。把编译放进 PR 门禁，问题在合并前暴露。

### 8.2 发布流水线（`.github/workflows/release.yml`）

触发：推送 `v*` tag（或 `workflow_dispatch` 手动触发预发布）。

```
tag v0.1.0
  └─ build (matrix: windows / macos-universal / ubuntu-22.04)
       ├─ tauri-action：编译 + 打包（NSIS / dmg / AppImage + deb）
       ├─ 签名：macOS codesign + notarytool；Windows 可选 Authenticode
       ├─ 生成 updater 产物与 latest.json（TAURI_SIGNING_PRIVATE_KEY）
       └─ 创建 **draft** Release，上传全部产物
  └─ 人工校验 draft → 手动 publish（不做全自动发布）
```

**签名与密钥管理**（全部走 GitHub Secrets，禁止入库）：

| Secret | 用途 |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` / `_PASSWORD` | 自动更新签名（必填） |
| `APPLE_CERTIFICATE` / `_PASSWORD` / `APPLE_ID` / `APPLE_TEAM_ID` / `APPLE_PASSWORD` | macOS 签名与公证 |
| `WINDOWS_CERTIFICATE` / `_PASSWORD` | Authenticode（可选，可先用 SignPath.io 的开源免费签名） |

**Ubuntu 版本选择**：用 `ubuntu-22.04` 而非 `latest`，保证 glibc 基线为 2.35，向下兼容面更广；`ubuntu-24.04` 构建的产物在旧发行版上会直接无法启动。

### 8.3 分支保护（main）

- 禁止直接 push（含管理员）
- 必须通过 PR，且 **至少 1 个 review approval**
- 必须通过 required checks：`fmt`、`clippy`、`test (ubuntu/windows/macos)`、`frontend`、`deny`
- 必须为最新分支状态（`Require branches to be up to date`）
- 使用 **Squash merge**，启用 `Require linear history`
- 合并后自动删除分支
- 禁止 force push、禁止删除 main

---

## 9. 分支与协作规范

### 9.1 分支模型：Trunk-Based

- `main` 始终可发布，短生命周期分支，生命周期目标 ≤ 3 天。
- 命名：`feat/<scope>-<desc>`、`fix/<scope>-<desc>`、`chore/...`、`docs/...`、`ci/...`、`refactor/...`
- 紧急修复同样走 PR（`fix/hotfix-...`），仅在服务不可用等极端场景才允许直推并立即补 PR 说明。

### 9.2 PR 规则（硬性）

1. **一 PR 一事**。混合重构与功能变更的 PR 直接打回。
2. **有效 diff ≤ 400 行**（不含测试、生成代码、锁文件）。超限须拆分为堆叠 PR（stacked PR），并在描述中标注依赖顺序。
3. **必须包含测试**：新增逻辑要有单测；修 bug 要有能复现的回归测试。
4. **必须更新文档**：涉及架构或契约变更，同步 `docs/`；重大决策补一篇 ADR。
5. **禁止提交**：注释掉的代码、`dbg!`/`println!` 调试输出、密钥与本地路径、未使用的依赖。
6. **提交信息**遵循 Conventional Commits；PR 标题即最终 squash 提交信息。
7. **CI 全绿 + 自审 checklist 完成**后才请求 review。

### 9.3 PR 模板要素

变更说明（做什么/为什么）｜关联 Issue｜测试方式（如何验证）｜风险与回滚方案｜截图或日志（UI 变更必填）

---

## 10. 质量门禁

| 门禁 | 工具 | 阈值 |
|---|---|---|
| 格式 | `cargo fmt` | 零差异 |
| 静态检查 | `cargo clippy -D warnings` | 零告警 |
| 单测 | `cargo test` | 全绿；`qcl-core` / `qcl-net` 行覆盖 ≥ 70% |
| 前端类型 | `tsc --noEmit` | 零错误 |
| 依赖合规 | `cargo deny` | 许可证白名单；无已知 RUSTSEC 漏洞 |
| 依赖更新 | Dependabot | 每周分组 PR，CI 全绿自动合并（patch 级） |
| 发布体积 | CI 断言 | 超预算则 Release job 失败 |

**覆盖率的现实定位**：只对纯逻辑 crate 设阈值。`src-tauri` 与 UI 不做覆盖率要求，改为 E2E 冒烟 + 手工验收清单。

---

## 11. 性能与体积预算

| 项 | 预算 | 手段 |
|---|---|---|
| Release profile | `opt-level = "s"`、`lto = "fat"`、`codegen-units = 1`、`strip = true` | 体积优先；`panic = "abort"` 需先确认插件不依赖 `catch_unwind`，验证后再开 |
| 前端产物 | 首屏 JS ≤ 150 KB gzip | React 19 + 路由级 `lazy()` 代码分割 + 按需引入图标 |
| 启动路径 | 冷启动不阻塞 UI：manifest 请求异步化，先渲染本地缓存 | 首屏读缓存 → 后台刷新 |
| 下载吞吐 | 16 并发；单文件大流量走 Range | 避免小文件挤占连接池 |
| 事件频率 | 进度事件 200 ms 节流 | 防止 IPC 洪泛拖垮 WebView |

---

## 12. 风险登记

| # | 风险 | 影响 | 对策 |
|---|---|---|---|
| R1 | Mojang 接口变更（如版本号方案改 `26.x`） | 启动失败 | 完全 manifest 驱动；接口层加 `insta` 快照，变更即报警 |
| R2 | Windows 未签名 exe 被 Defender/SmartScreen 拦截 | 用户流失 | 尽早接入代码签名（SignPath 开源免费额度 / Azure Trusted Signing） |
| R3 | macOS 公证需要付费开发者账号 | 无法分发 | Phase 0 即确认账号状态；未就绪则先发未签名 dmg 并在 README 说明 `xattr` 处理 |
| R4 | WebView2 在旧版 Win10 缺失 | 启动白屏 | 使用 Tauri 的 `downloadBootstrapper` 安装模式 |
| R5 | Forge/NeoForge 安装器流程复杂（需先起 JVM 跑 installer） | 工期不可控 | **已决策支持（D1）**：拆为独立 PR 置于 Phase 4 末尾，Fabric/Quilt 先行交付；若显著超期则先发布不含 Forge 的版本 |
| R6 | 微软 OAuth 需要注册 Azure 应用（client_id） | 阻塞登录 | 立即注册（`consumers` 租户、允许公共客户端流、无需 secret） |
| R7 | 中文/空格路径导致 JVM 参数解析失败 | 区域性高频问题 | 全程参数数组传参；CI 增加含空格与中文的路径用例 |
| R8 | 首次下载 500 MB 体验差 | 留存 | 分阶段进度 + 可中断续传 + 复用已有 `.minecraft` 资源（去重校验） |
| R9 | GPL-3.0 传染性影响未来商业化 | 法律 | 立项即明确：保持 GPL-3.0；若未来需闭源，须在贡献者协议层面提前规划 |

---

## 13. 立即可执行的下一步

1. **补齐本机环境**：当前工作区未检测到 Rust 工具链（`rustc`/`cargo` 均不在 PATH），需先安装 `rustup` + MSVC Build Tools，并确认 `cargo tauri` 可用。
2. **合并本规划**：以 `docs/development-plan` 分支提 PR，评审通过后 squash 合并。
3. **执行 Phase 0 / PR #1**：仓库基建（模板、规范、工具链固定）。
4. **执行 Phase 0 / PR #2–#3**：CI 门禁与三平台打包链路验证。**在写任何业务代码之前，先证明发布链路是通的。**
5. **注册 Azure 应用**（R6），拿到 `client_id` 备用；同步确认 Apple 开发者账号与 Windows 签名方案（R2/R3）。

---

## 附：决策记录索引

| ADR | 主题 | 状态 |
|---|---|---|
| ADR-0001 | 选择 Tauri 2 而非 Electron | 待撰写（Phase 0） |
| ADR-0002 | 前端选用 React 19 | 待撰写（Phase 0） |
| ADR-0003 | 配置用 JSON 而非 SQLite | 待撰写（Phase 2） |
| ADR-0004 | 认证采用设备码流程 | 待撰写（Phase 3） |
