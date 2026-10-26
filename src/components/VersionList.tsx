// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import type { VersionSummary } from "../api";
import { formatDate, typeLabel } from "../lib/format";

/**
 * 版本列表。
 *
 * 不画卡片外框：外框由布局提供（左栏自带右边框），组件只负责列表本身。
 * 这样同一个列表既能贴边、也能放进弹层，不必为容器写两套样式。
 */
export function VersionList({
  versions,
  selected,
  disabled,
  onSelect,
}: {
  versions: VersionSummary[];
  selected: string | null;
  /** 安装或运行时禁止切换版本，避免把进度与目标版本对错。 */
  disabled: boolean;
  onSelect: (id: string) => void;
}) {
  if (versions.length === 0) {
    return (
      <p className="px-4 py-6 text-xs text-[var(--color-text-secondary)]">没有可用版本。</p>
    );
  }

  return (
    <ul className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
      {versions.map((version) => {
        const active = version.id === selected;
        return (
          <li key={version.id}>
            <button
              type="button"
              disabled={disabled}
              aria-pressed={active}
              onClick={() => onSelect(version.id)}
              className={`group relative flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left transition-colors disabled:cursor-not-allowed ${
                active
                  ? "bg-[var(--color-accent-soft)]"
                  : "hover:bg-[var(--color-surface-hover)] disabled:opacity-50"
              }`}
            >
              {/* 选中态用左侧竖条，比整块变色更克制，也更容易在长列表里扫到。 */}
              <span
                className={`absolute left-0 top-1/2 h-5 w-[2px] -translate-y-1/2 rounded-full transition-opacity ${
                  active ? "bg-[var(--color-accent)] opacity-100" : "opacity-0"
                }`}
              />
              <span
                className={`truncate font-mono text-xs ${
                  active
                    ? "text-[var(--color-accent)]"
                    : "text-[var(--color-text-primary)]"
                }`}
              >
                {version.id}
              </span>
              <span className="ml-auto flex shrink-0 items-center gap-1.5">
                {/* 只标非正式版：清单里绝大多数是 release，逐行重复「正式版」纯属噪音。 */}
                {version.type !== "release" && (
                  <span className="rounded bg-[var(--color-surface-hover)] px-1.5 py-0.5 text-[10px] text-[var(--color-warning)]">
                    {typeLabel(version.type)}
                  </span>
                )}
                <span className="w-[74px] text-right text-[10px] tabular-nums text-[var(--color-text-muted)]">
                  {formatDate(version.releaseTime)}
                </span>
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}
