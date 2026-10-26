// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import type { LogEntry } from "./logs";

/**
 * 崩溃摘要：从整段日志里挑出「值得看的那几行」。
 *
 * 为什么需要：游戏异常退出时日志动辄几千行，用户要么全贴给别人，要么自己在里面
 * 翻。真正有用的信息其实集中在少数几行——模组依赖冲突、OOM、主类找不到。
 *
 * 纯函数，不碰 DOM 也不碰 IPC：规则的正确性可以逐条测。
 */

/** 游戏自己写下的崩溃报告（由后端读出头部）。 */
export interface CrashReport {
  /** 报告文件路径。 */
  path: string;
  /** `Description:` 那一行。 */
  description: string | null;
  /** 结论下方紧接着的几行（异常类型 + 顶部堆栈帧）。 */
  excerpt: string[];
}

/** 一次异常退出的摘要。 */
export interface CrashSummary {
  /** 退出码。 */
  exitCode: number;
  /** 一句话结论。 */
  headline: string;
  /** 关键日志行（已去重、已截断）。 */
  lines: string[];
  /** 崩溃报告路径；这一局没写报告时为 `null`。 */
  reportPath: string | null;
}

/** 摘要里最多保留几行——再多就不叫摘要了。 */
const MAX_LINES = 12;

/**
 * 与「游戏崩溃」无关、但每次离线启动都会出现的噪音。
 *
 * 这些是离线账户的必然结果（没有正版令牌），把它们的 401 当成崩溃原因会误导人。
 */
const NOISE: RegExp[] = [
  /Failed to fetch user properties/i,
  /Failed to fetch Realms feature flags/i,
  /Couldn't connect to realms/i,
  /RealmsServiceException/i,
  /InvalidCredentialsException/i,
  /SignedJWT/i,
  /^\s+at\s/, // 堆栈帧本身没信息量，先不收
];

/** 模组加载阶段的失败：这类问题最需要优先呈现。 */
const MOD_PATTERNS: RegExp[] = [
  /Incompatible mod set/i,
  /Mod resolution/i,
  /requires .* which is missing/i,
  /Missing or unsupported mandatory dependencies/i,
  /failed to load mod/i,
  /Caused by: .*ModResolutionException/i,
];

/** 内存不足。 */
const OOM_PATTERNS: RegExp[] = [/OutOfMemoryError/i, /Java heap space/i, /insufficient memory/i];

/** 一般的错误行。 */
const ERROR_PATTERNS: RegExp[] = [
  /\bFATAL\b/,
  /\bERROR\b/,
  /Caused by:/,
  /Exception/,
  /Could not find or load main class/i,
  /UnsupportedClassVersionError/i,
  /GLFW error/i,
  /Failed to (start|launch|initialize)/i,
];

function matches(line: string, patterns: RegExp[]): boolean {
  return patterns.some((pattern) => pattern.test(line));
}

function isNoise(line: string): boolean {
  return matches(line, NOISE);
}

/**
 * 生成崩溃摘要。
 *
 * `report` 是游戏自己写在 `crash-reports/` 里的报告（后端已解析出头部）。它是
 * 最权威的一手材料，但**不能只看它**：Fabric 的模组依赖冲突根本走不到写报告那一步，
 * 只在日志里留几行。所以两边都要用。
 *
 * 返回 `null` 表示「不是崩溃」：正常退出（0）、被用户终止（没有退出码），
 * 或者日志里确实没有任何值得说的错误。
 */
export function summarizeCrash(
  logs: readonly LogEntry[],
  exitCode: number | null,
  report: CrashReport | null = null,
): CrashSummary | null {
  // 被「停止」终止时没有退出码，那不是崩溃。
  if (exitCode === null || exitCode === 0) {
    return null;
  }

  const candidates = logs
    .map((entry) => entry.text.trim())
    .filter((text) => text.length > 0 && !isNoise(text));

  const modLines = candidates.filter((text) => matches(text, MOD_PATTERNS));
  const oomLines = candidates.filter((text) => matches(text, OOM_PATTERNS));
  const errorLines = candidates.filter((text) => matches(text, ERROR_PATTERNS));

  // 日志这边仍按优先级二选一：模组冲突的现场里还混着几十条无关的 ERROR，
  // 全收进来就把真正的原因淹了。
  const chosen = modLines.length > 0 ? modLines : oomLines.length > 0 ? oomLines : errorLines;

  // 报告里的堆栈放最前面：那是游戏自己认定的现场。
  const lines = dedupe([...(report?.excerpt ?? []), ...chosen]).slice(0, MAX_LINES);

  if (lines.length === 0) {
    // 日志里没有任何可识别的原因，也要告诉用户「退出了」，并给出退出码。
    return {
      exitCode,
      headline: `游戏异常退出（退出码 ${exitCode}），日志里没有可识别的错误`,
      lines: [],
      reportPath: report?.path ?? null,
    };
  }

  // 结论的优先级：模组冲突 > 内存不足 > 报告里的 Description > 泛泛而谈。
  //
  // 前两类是用户能自己动手解决的，比报告里那句「Rendering overlay」有用得多；
  // 但报告没有提供描述时也不能空着。
  const headline =
    modLines.length > 0
      ? "模组依赖或兼容性有问题，游戏在加载阶段退出"
      : oomLines.length > 0
        ? "内存不足，尝试调低模组数量或在设置里加大内存"
        : report?.description
          ? `游戏崩溃：${report.description}`
          : `游戏异常退出（退出码 ${exitCode}）`;

  return { exitCode, headline, lines, reportPath: report?.path ?? null };
}

/** 去重但保持原顺序：同一段堆栈重复出现时只留一次。 */
function dedupe(lines: string[]): string[] {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const line of lines) {
    if (!seen.has(line)) {
      seen.add(line);
      result.push(line);
    }
  }
  return result;
}

/** 把摘要拼成可复制的文本。 */
export function formatSummary(summary: CrashSummary): string {
  const header = `${summary.headline}（退出码 ${summary.exitCode}）`;
  const parts = summary.lines.length === 0 ? [header] : [header, "", ...summary.lines];
  // 报告路径也带上：贴到论坛求助时，别人能直接指路去看完整内容。
  if (summary.reportPath) {
    parts.push("", `崩溃报告：${summary.reportPath}`);
  }
  return parts.join("\n");
}
