// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

/**
 * 展示层的格式化逻辑。
 *
 * 抽成纯函数而不是留在组件里，是为了能直接测——组件测试要跑渲染，
 * 而这些规则（日期格式、类型标签）本身与 DOM 无关。
 */

/** 版本类型的中文标签。未知取值回退到原始字符串。 */
const TYPE_LABELS: Record<string, string> = {
  release: "正式版",
  snapshot: "快照",
  old_beta: "Beta",
  old_alpha: "Alpha",
  other: "其他",
};

/** 取版本类型的中文标签。 */
export function typeLabel(type: string): string {
  return TYPE_LABELS[type] ?? type;
}

/**
 * 把 ISO 时间格式化成 `YYYY/MM/DD`。
 *
 * 解析失败时原样返回：清单里的时间理论上都是 ISO，但显示层不该因为一条脏数据
 * 就抛错或显示 `Invalid Date`。
 */
export function formatDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) {
    return iso;
  }
  return date.toLocaleDateString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  });
}

/** 安装阶段的标签。 */
const PHASE_LABELS: Record<string, string> = {
  versionFiles: "版本文件",
  assets: "资产",
};

/** 取安装阶段的中文标签。 */
export function phaseLabel(phase: string): string {
  return PHASE_LABELS[phase] ?? phase;
}
