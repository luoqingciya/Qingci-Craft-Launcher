/**
 * 与 Rust 侧 IPC 的类型契约。
 *
 * 这里手写类型而不生成：命令只有寥寥几个，手写反而更容易看清字段含义。
 * 一旦命令数量上来（安装、启动、日志流），应当改为从 Rust 类型生成。
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

/** 拉取官方版本清单。 */
export function listVersions(): Promise<VersionSummary[]> {
  return invoke<VersionSummary[]>("list_versions");
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
