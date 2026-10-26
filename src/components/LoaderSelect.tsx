// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import type { LoaderOption } from "../api";

/**
 * 加载器选择。
 *
 * 抽成独立组件的理由：它的状态组合（读取中 / 空列表 / 有测试版 / 禁用）
 * 比它看起来的样子多，而这些组合都值得单独验证，不必拉起整个 App。
 */
export function LoaderSelect({
  loaders,
  value,
  busy,
  disabled,
  onChange,
}: {
  loaders: LoaderOption[];
  value: string;
  busy: boolean;
  disabled: boolean;
  onChange: (value: string) => void;
}) {
  return (
    <label className="flex items-center gap-2 text-xs text-[var(--color-text-secondary)]">
      加载器
      <select
        aria-label="加载器"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        disabled={disabled || busy}
        className="rounded border border-[var(--color-border-subtle)] bg-[var(--color-surface)] px-2 py-1 font-mono text-xs"
      >
        <option value="">无（原版）</option>
        {loaders.map((option) => (
          <option key={option.version} value={option.version}>
            Fabric {option.version}
            {option.stable ? "" : "（测试版）"}
          </option>
        ))}
      </select>
      {busy && <span>读取中…</span>}
    </label>
  );
}
