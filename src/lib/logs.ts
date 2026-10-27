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

/** 日志级别。由文本推导，不额外存字段——存了就要在每条追加路径上维护它。 */
export type LogLevel = "info" | "warn" | "error";

/**
 * 判断一条日志的级别。
 *
 * 依据是 Minecraft 自己的格式 `[12:04:34] [Render thread/WARN]: ...`，
 * 以及 stderr 这个兜底信号。刻意不追求穷尽：判错的代价只是筛选时多一行少一行，
 * 而过度匹配（比如把包含 "error" 的普通句子当错误）会让筛选变得不可信。
 */
export function levelOf(entry: LogEntry): LogLevel {
  // 先认 Minecraft / log4j 的 `[线程/级别]` 标记。这是最强的信号——有些 WARN
  // 走的是 stderr，若先按流判断就会把它误判成错误（预览页那条 Realms 警告
  // 就是这么被吃掉、点「警告」一行都筛不出来的）。
  const tagged = entry.text.match(/\/(WARN|ERROR|FATAL|SEVERE)\]/);
  if (tagged) {
    return tagged[1] === "WARN" ? "warn" : "error";
  }

  // 没有标记时退回关键词。用词边界避免 "terror" 误命中；大小写不敏感，
  // 因为模组自己写的 "Warning:" 也是小写开头。
  if (/\b(FATAL|SEVERE|ERROR)\b/i.test(entry.text)) {
    return "error";
  }
  if (/\bWARN(ING)?\b/i.test(entry.text)) {
    return "warn";
  }

  // 最后才看流：stderr 是「没写级别、但走了错误流」的兜底。
  if (entry.stream === "stderr") {
    return "error";
  }
  return "info";
}

/** 按级别与关键词筛选日志。关键词为空表示不按内容过滤。 */
export function filterLogs(
  lines: readonly LogEntry[],
  level: LogLevel | "all",
  query: string,
): LogEntry[] {
  const needle = query.trim().toLowerCase();
  return lines.filter((line) => {
    if (level !== "all" && levelOf(line) !== level) {
      return false;
    }
    return needle === "" || line.text.toLowerCase().includes(needle);
  });
}
