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
}

/** 提交给 Rust 的设置。空字符串表示「用默认值」。 */
export interface SettingsInput {
  gameDir: string | null;
  playerName: string | null;
  memoryMb: number | null;
  javaPath: string | null;
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

/** 游戏进程退出。 */
export interface GameExitEvent {
  /** 退出码；被信号终止时为 `null`。 */
  code: number | null;
  durationSecs: number;
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

/** 启动游戏。立即返回，日志与退出通过事件推送。 */
export function launchGame(version: string): Promise<void> {
  return invoke<void>("launch_game", { version });
}

/** 请求停止游戏。 */
export function stopGame(): Promise<void> {
  return invoke<void>("stop_game");
}

/** 查询运行状态。 */
export function gameStatus(): Promise<GameStatus> {
  return invoke<GameStatus>("game_status");
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
}

/**
 * 搜索模组。
 *
 * `loader` 传 `fabric` 这类分类名；空串表示不按加载器筛选。
 */
export function searchMods(
  query: string,
  version: string,
  loader: string,
): Promise<ModHit[]> {
  return invoke<ModHit[]>("search_mods", { query, version, loader });
}

/** 安装模组，返回落地路径（相对游戏目录）。 */
export function installMod(
  projectId: string,
  version: string,
  loader: string,
): Promise<string> {
  return invoke<string>("install_mod", { projectId, version, loader });
}

/** 列出已安装的模组文件名。 */
export function listMods(): Promise<string[]> {
  return invoke<string[]>("list_mods");
}
