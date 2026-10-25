// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

/**
 * 日志缓冲。
 *
 * 抽成纯函数的原因：截断规则（保留最近 N 行）是容易写错又难在界面上验证的逻辑，
 * 用纯函数测比渲染组件可靠得多。
 */

/** 日志来源。`system` 是界面自己插入的分隔信息（如「游戏退出」），不来自进程。 */
export type LogStreamKind = "stdout" | "stderr" | "system";

/** 带稳定 key 的日志行。 */
export type LogEntry = { id: number; stream: LogStreamKind; text: string };

/** 保留的最大行数：模组报错循环时游戏可以每秒刷出上千行，无界增长会把内存吃光。 */
export const MAX_LINES = 2000;

/**
 * 追加一行并截断到上限。
 *
 * 返回新数组（不修改入参）：React 依赖引用变化判断是否重渲染，
 * 原地 `push` 会导致界面不更新。
 */
export function appendLog(
  lines: readonly LogEntry[],
  entry: LogEntry,
  max: number = MAX_LINES,
): LogEntry[] {
  const next = [...lines, entry];
  if (max <= 0) {
    return [];
  }
  return next.length > max ? next.slice(next.length - max) : next;
}
