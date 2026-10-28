// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

/**
 * 与 Rust 侧 IPC 的类型契约。
 *
 * 这里手写类型而不生成：命令只有寥寥几个，手写反而更容易看清字段含义。
 * 命令数量再涨（账号、模组、设置项）时应当改为从 Rust 类型生成。
 */
import { invoke } from "@tauri-apps/api/core";

/** 版本类型。官方取值之外一律归为 `other`。 */
export type VersionType = "release" | "snapshot" | "old_beta" | "old_alpha" | "other";

/** 版本清单条目。 */
export interface VersionSummary {
  id: string;
  type: VersionType;
  /** ISO 8601 发布时间。 */
  releaseTime: string;
}

/** 设置视图。字段与 Rust 侧 `SettingsView` 一一对应。 */
export interface SettingsView {
  /** 当前生效的游戏目录。 */
  gameDir: string;
  /** 默认目录（安装目录下），供「恢复默认」使用。 */
  defaultGameDir: string;
  /** 是否使用了自定义目录。 */
  isCustom: boolean;
  /** 默认目录不可用时的回退原因。 */
  fallbackReason: string | null;
  /** 实际使用的玩家名（已回退到默认值）。 */
  playerName: string;
  /** 最大堆内存（MB）；`null` 表示采用官方默认值。 */
  memoryMb: number | null;
  /** 显式指定的 Java 可执行文件；`null` 表示自动选择。 */
  javaPath: string | null;
  /** 允许的内存范围 `[最小, 最大]`，前端据此校验。 */
  memoryRange: [number, number];
  /** 上次选择的版本。 */
  selectedVersion: string | null;
  /** 上次选择的加载器版本。 */
  loaderVersion: string | null;
  /** 下载源：`official` 或 `bmclapi`。 */
  downloadSource: string;
  /** 已知实例，按添加顺序排列。 */
  instances: InstanceView[];
  /** 当前实例名。 */
  activeInstance: string;
}

/** 一个实例：一套独立的游戏目录。 */
export interface InstanceView {
  /** 显示名，同时是标识。 */
  name: string;
  /** 实际生效的游戏目录。 */
  gameDir: string;
  /** 是否跟随安装位置（默认目录）。 */
  isDefaultDir: boolean;
  /** 是否为当前实例。 */
  active: boolean;
  /** 该实例上次选择的版本。 */
  selectedVersion: string | null;
  /** 该实例上次选择的加载器版本。 */
  loaderVersion: string | null;
}

/** 提交给 Rust 的设置。空字符串表示「用默认值」。 */
export interface SettingsInput {
  gameDir: string | null;
  playerName: string | null;
  memoryMb: number | null;
  javaPath: string | null;
  /** 下载源：`official` / `bmclapi`；`null` 表示官方。 */
  downloadSource: string | null;
}

/** 一个可用的 Java 安装。 */
export interface JavaOption {
  /** 可执行文件路径，同时作为选项的值。 */
  path: string;
  /** 版本字符串，如 `26.0.2`。 */
  version: string;
  /** 主版本号。 */
  major: number;
  /** 来源描述，如「JAVA_HOME」。 */
  source: string;
}

/** 安装结果摘要。 */
export interface InstallSummary {
  version: string;
  /** 版本文件计划数。 */
  files: number;
  /** 实际下载数。 */
  downloaded: number;
  /** 跳过数（已存在且校验通过）。 */
  skipped: number;
  /** 资产对象数。 */
  assets: number;
  /** 解压出的 natives 文件数。 */
  natives: number;
}

/** 安装进度事件。 */
export interface InstallProgressEvent {
  /** `versionFiles` 或 `assets`。 */
  phase: "versionFiles" | "assets";
  completed: number;
  total: number;
  percent: number;
}

/** 一行游戏日志。 */
export interface GameLogEvent {
  stream: "stdout" | "stderr";
  text: string;
}

/** 游戏自己写下的崩溃报告（后端只读头部，不整份传）。 */
export interface CrashReport {
  /** 报告文件路径。 */
  path: string;
  /** `Description:` 那一行。 */
  description: string | null;
  /** 结论下方紧接着的几行。 */
  excerpt: string[];
}

/** 游戏进程退出。 */
export interface GameExitEvent {
  /** 退出码；被信号终止时为 `null`。 */
  code: number | null;
  durationSecs: number;
  /** 本次运行产生的崩溃报告；正常退出或没写报告时为 `null`。 */
  crashReport: CrashReport | null;
}

/** 运行状态。 */
export interface GameStatus {
  running: boolean;
  version: string | null;
}

/** 事件名，与 Rust 侧常量保持一致。 */
export const INSTALL_PROGRESS_EVENT = "install-progress";
export const GAME_LOG_EVENT = "game-log";
export const GAME_EXIT_EVENT = "game-exit";

/** 拉取官方版本清单。 */
export function listVersions(): Promise<VersionSummary[]> {
  return invoke<VersionSummary[]>("list_versions");
}

/** 读取当前设置。 */
export function getSettings(): Promise<SettingsView> {
  return invoke<SettingsView>("get_settings");
}

/** 保存设置。整体提交，一次写清。 */
export function saveSettings(input: SettingsInput): Promise<SettingsView> {
  return invoke<SettingsView>("save_settings", { input });
}

/** 新建实例并切换过去。目录为空时落在 `<安装目录>/instances/<名字>`。 */
export function createInstance(name: string, dir: string | null): Promise<SettingsView> {
  return invoke<SettingsView>("create_instance", { name, dir });
}

/** 切换到另一个实例。 */
export function switchInstance(name: string): Promise<SettingsView> {
  return invoke<SettingsView>("switch_instance", { name });
}

/** 重命名实例。名字是标识，所以当前实例指针会跟着改。 */
export function renameInstance(from: string, to: string): Promise<SettingsView> {
  return invoke<SettingsView>("rename_instance", { from, to });
}

/** 从列表里移除实例。**不删文件**——游戏目录里可能有存档。 */
export function deleteInstance(name: string): Promise<SettingsView> {
  return invoke<SettingsView>("delete_instance", { name });
}

/** 列出本机可用的 Java。 */
export function listJava(): Promise<JavaOption[]> {
  return invoke<JavaOption[]>("list_java");
}

/** 一个可选的加载器版本。 */
export interface LoaderOption {
  /** 加载器版本号，如 `0.20.1`。 */
  version: string;
  /** 是否为稳定版。 */
  stable: boolean;
}

/** 安装指定版本。幂等，可重复调用。`loader` 为 `null` 时安装原版。 */
export function installGameVersion(version: string, loader: string | null): Promise<InstallSummary> {
  return invoke<InstallSummary>("install_game_version", { version, loader });
}

/** 列出某个游戏版本可用的 Fabric 加载器版本。 */
export function listLoaders(version: string): Promise<LoaderOption[]> {
  return invoke<LoaderOption[]>("list_loaders", { version });
}

/**
 * 启动游戏。立即返回，日志与退出通过事件推送。
 *
 *  与安装时保持一致：后端据此推导目标版本（原版或合并后的加载器版本），
 * 不再依赖界面上的临时状态。
 */
export function launchGame(version: string, loader: string | null): Promise<void> {
  return invoke<void>("launch_game", { version, loader });
}

/** 请求停止游戏。 */
export function stopGame(): Promise<void> {
  return invoke<void>("stop_game");
}

/** 查询运行状态。 */
export function gameStatus(): Promise<GameStatus> {
  return invoke<GameStatus>("game_status");
}

/** 运行环境信息。 */
export interface RuntimeInfo {
  /** 是否由安装程序装入。便携版没有卸载器，因此无法自更新。 */
  installed: boolean;
  /** 本次构建是否内置了微软登录所需的客户端 ID。 */
  msaLogin: boolean;
  /** 应用版本。 */
  appVersion: string;
  /** 编译时链接的 Tauri 版本。 */
  tauriVersion: string;
  /** 目标系统，如 windows。 */
  os: string;
  /** 目标架构，如 x86_64。 */
  arch: string;
}

/** 查询运行环境。 */
export function runtimeInfo(): Promise<RuntimeInfo> {
  return invoke<RuntimeInfo>("runtime_info");
}

/** 在文件管理器里选中崩溃报告。 */
export function revealCrashReport(path: string): Promise<void> {
  return invoke<void>("reveal_crash_report", { path });
}

/** 模组搜索结果。 */
export interface ModHit {
  /** 项目 id，安装时回传。 */
  projectId: string;
  /** 标题。 */
  title: string;
  /** 一句话描述。 */
  description: string;
  /** 下载量。 */
  downloads: number;
  /** 作者。 */
  author: string;
  /** 图标地址；项目没上传图标时为 null。 */
  iconUrl: string | null;
  /** 项目短名，用于拼 Modrinth 页面地址。 */
  slug: string;
  /** 中文译名；译名库里没有时为 null。 */
  zhName: string | null;
}

/** 一个可筛选的内容分类。 */
export interface ModCategory {
  /** Modrinth 的分类 id，如 `optimization`。 */
  id: string;
  /** 中文标签。 */
  label: string;
}

/** 搜索结果：附带实际使用的查询词。 */
export interface SearchOutcome {
  /** 实际发给 Modrinth 的查询词（中文会被换成英文关键词）。 */
  query: string;
  /** 命中的项目。 */
  hits: ModHit[];
}

/**
 * 搜索模组。
 *
 * `loader` 传 `fabric` 这类分类名；空串表示不按加载器筛选。
 * `categories` 是内容分类，多个之间是「或」。
 */
export function searchMods(
  query: string,
  version: string,
  loader: string,
  categories: string[],
): Promise<SearchOutcome> {
  return invoke<SearchOutcome>("search_mods", { query, version, loader, categories });
}

/** 列出可用于筛选的模组分类。 */
export function listModCategories(): Promise<ModCategory[]> {
  return invoke<ModCategory[]>("list_mod_categories");
}

/** 安装模组及其必需依赖，返回本次实际落地的路径列表。 */
export function installMod(
  projectId: string,
  version: string,
  loader: string,
): Promise<string[]> {
  return invoke<string[]>("install_mod", { projectId, version, loader });
}

/** 一个已安装的模组。 */
export interface InstalledMod {
  /**  下的文件名。 */
  fileName: string;
  /** Modrinth 项目 id；手动放进 mods/ 的 jar 为 null。 */
  projectId: string | null;
  /** 安装时的版本号；未知来源时为 null。 */
  versionNumber: string | null;
}

/** 列出已安装的模组。带来源信息，界面据此判断搜索结果装没装过。 */
export function listMods(): Promise<InstalledMod[]> {
  return invoke<InstalledMod[]>("list_mods");
}

/** 一个必需依赖。 */
export interface ModDependency {
  /** Modrinth 项目 id。 */
  projectId: string;
  /** 项目标题；查不到时为 null。 */
  title: string | null;
  /** 是否已经装上了。 */
  installed: boolean;
}

/**
 * 查一个模组的必需依赖。
 *
 * 界面上是「点开才查」：每个项目要一次 version 请求，塞进搜索会拖慢列表。
 */
export function modDependencies(
  projectId: string,
  version: string,
  loader: string,
): Promise<ModDependency[]> {
  return invoke<ModDependency[]>("mod_dependencies", { projectId, version, loader });
}

/** 删除一个已安装的模组。只接受 mods/ 下的 .jar 文件名。 */
export function removeMod(filename: string): Promise<void> {
  return invoke<void>("remove_mod", { filename });
}

/**
 * 记住当前选择（版本 + 加载器）。
 *
 * 与 `saveSettings` 分开：切换版本是高频操作，不该连带把设置表单里
 * 还没提交的改动写进去。
 */
export function rememberSelection(version: string | null, loader: string | null): Promise<void> {
  return invoke<void>("remember_selection", { version, loader });
}

/** 一个可更新的模组。 */
export interface ModUpdate {
  /** 已安装的文件名。 */
  fileName: string;
  /** Modrinth 项目 id。 */
  projectId: string;
  /** 当前版本号。 */
  currentVersion: string;
  /** 最新版本号。 */
  latestVersion: string;
  /** 最新版本的文件名。 */
  latestFileName: string;
}

/** 检查已安装模组是否有新版本。 */
export function checkModUpdates(version: string, loader: string): Promise<ModUpdate[]> {
  return invoke<ModUpdate[]>("check_mod_updates", { version, loader });
}

/** 更新一个模组：装最新版本并删掉旧文件，返回本次落地的路径。 */
export function updateMod(
  projectId: string,
  oldFile: string,
  version: string,
  loader: string,
): Promise<string[]> {
  return invoke<string[]>("update_mod", { projectId, oldFile, version, loader });
}

/** 登录后的账号。令牌不会传到前端。 */
export interface Account {
  /** 玩家名。 */
  name: string;
  /** 账号 UUID（无连字符）。 */
  uuid: string;
}

/** 登录状态推送的事件名。 */
export const LOGIN_EVENT = "login://state";

/**
 * 登录状态。
 *
 * 三种状态都由后端推过来：轮询节奏（`slow_down` 要加间隔、到点要停）属于协议细节，
 * 放在前端就要复制一份，两边不一致会出现「微软那边已授权、启动器还在转圈」。
 */
export type LoginState =
  | {
      kind: "code";
      /** 要用户输入的短码。 */
      userCode: string;
      /** 要用户访问的地址。 */
      verificationUri: string;
      /** 短码有效期（秒）。 */
      expiresIn: number;
    }
  | { kind: "done"; account: Account }
  | { kind: "failed"; message: string };

/** 取当前账号，未登录时为 `null`。 */
export function currentAccount(): Promise<Account | null> {
  return invoke<Account | null>("current_account");
}

/** 开始设备码登录。结果通过 {@link LOGIN_EVENT} 推送。 */
export function beginLogin(): Promise<void> {
  return invoke<void>("begin_login");
}

/** 取消正在进行的登录。 */
export function cancelLogin(): Promise<void> {
  return invoke<void>("cancel_login");
}

/** 退出登录。 */
export function signOut(): Promise<void> {
  return invoke<void>("sign_out");
}
