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

/**
 * 设置游戏目录。
 *
 * `path` 传 `null` 表示恢复默认（安装目录下的 `game`）。
 */
export function setGameDir(path: string | null): Promise<SettingsView> {
  return invoke<SettingsView>("set_game_dir", { path });
}

/** 安装指定版本。幂等，可重复调用。 */
export function installGameVersion(version: string): Promise<InstallSummary> {
  return invoke<InstallSummary>("install_game_version", { version });
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
