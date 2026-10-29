# ADR-0005：加载器安装走自研 processor 链

- 状态：已接受
- 日期：2026-10-29
- 决策者：@luoqingciya

## 背景

Fabric 只需要合并一份 profile，是纯 JSON 操作。Forge / NeoForge 不是——安装器要真的跑一遍
步骤：切分原版 jar、把 MCP 与 Mojang 的 mappings 合并、重映射、打二进制补丁，最后写出
`versions/<id>/<id>.json`。

最省事的做法是调官方安装器（`java -jar forge-installer.jar --installClient <dir>`），
一开始也确实这么做的。它有三个绕不过去的问题：

1. **`--offline` 关不掉 processor 的网络**。processor 是安装器 fork 出来的独立 JVM，
   其中 `DOWNLOAD_MOJMAPS` 把 `launchermeta.mojang.com` 写死在字节码里，而且**先把输出
   删掉再下载**——预置文件也无效。
2. **`--mirror` 只覆盖 maven**，管不到上面那个地址。
3. **失败信息不指位置**：缺一个库只报「这些库下载失败」，缺一个 processor jar 只报
   `Missing Jar for processor`，都不说是哪个。

国内网络下 `launchermeta.mojang.com` 经常不可达，于是安装要么卡十几分钟，要么直接失败。

## 决策

**自己编排 processor 步骤，不跑官方安装器。** 但**不重写 processor 实现**——
`install_profile.json` 已经把每一步的参数写清楚了：

| 步骤 | 谁来跑 |
| --- | --- |
| `MCP_DATA` / `MERGE_MAPPING` / `jarsplitter` / `AutoRenamingTool` / `SpecialSource` / `binarypatcher` | 原样跑官方 processor（`java -cp … <Main-Class> …`） |
| `DOWNLOAD_MOJMAPS` | **换成本项目下载器**：地址取自本地 `versions/<mc>/<mc>.json` 的 `downloads.client_mappings`，可走镜像、可校验 SHA-1 |
| `EXTRACT_FILES` / `BUNDLER_EXTRACT` | `sides` 里没有 `client`，客户端直接跳过 |

1.12.2 及更早的 profile **没有 `processors`**（只有一个 `install` 段，把内嵌的 universal jar
复制成 `versions/<id>/<id>.jar`），这部分仍交给官方安装器，按 `has_processors` 自动分流。

同时补上 `inheritsFrom` 合并：安装器写出的 JSON 只有增量（29 个库、没有 `assetIndex`、
没有 `downloads`、游戏参数里没有 `--username`），不合并就无法启动。

## 理由

1. **联网只剩一步，且那一步可控**：除了 mappings，全部本地执行；mappings 走本项目下载器，
   于是镜像、重试、SHA-1 校验、错误归因全部复用同一套逻辑。
2. **产物与官方安装器逐字节一致**：`install_profile` 里声明了 `MC_SLIM_SHA` / `MC_EXTRA_SHA` /
   `PATCHED_SHA`，实测三者全部吻合。这不是「看起来能跑」，是可验证的等价。
3. **不重写 processor 是对的**：tsrg 合并、二进制补丁这些格式没有重写的价值，
   而编排（谁先谁后、参数怎么替换）才是安装器真正的复杂度所在。
4. **错误能归因**：哪一步、哪个 jar、哪个坐标，都在我们自己的代码里，可以给出具体信息。

## 后果

- **必须理解 `install_profile` 的三种「反直觉」形态**，否则会踩一串坑（都已在代码注释里记下）：
  - `data` 段里的 `[坐标]` 是 processor 的**输出路径**，不是下载项
  - 1.13–1.16 的安装器把 Forge 自己的 jar 塞在内嵌 `maven/` 树里，`url: ""` 的条目靠它满足
  - 库合并要按 `group:artifact` **替换**（Forge 用 log4j 2.15.0 顶掉原版的 2.8.1）
- **processor 仍是独立 JVM**，所以安装加载器需要一个 **Java 8+** 运行时（与游戏要求的 Java 无关）。
- **每个「游戏版本 + 加载器版本」各存一份客户端 jar**（约 23 MB）：版本目录是独立的，
  classpath 末尾的 `<id>.jar` 必须存在。与 Fabric 的既有取舍一致。
- **1.13–1.19 之间的版本结构有差异**（例如 1.16.5 用 `SpecialSource` 而不是
  `AutoRenamingTool`、没有 `MERGE_MAPPING`），链要按 profile 驱动而不是按版本号分支。
  已验证 Forge 1.16.5 / 1.20.1 与 NeoForge 21.1.72。

## 替代方案

- **官方安装器 + 代理**：现状。能用，但把「能不能装」寄托在用户配好代理上，
  且错误信息无法归因。`--offline` 也不解决问题（见背景第 1 条）。
- **完全重写 processor**：要自己实现 tsrg 合并、SRG 重映射、二进制补丁格式，
  工作量与风险都远大于收益。
- **复用第三方启动器的实现**（如 HMCL 的安装逻辑）：许可证与维护成本都不合适，
  且会把上游的假设一起带进来。
