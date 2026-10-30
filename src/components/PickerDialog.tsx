// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { useEffect, useMemo, useRef, useState } from "react";

/** 选择器里的一项。 */
export interface PickerItem {
  /** 唯一标识，同时作为回传值。 */
  id: string;
  /** 主文案，等宽显示（版本号、加载器版本）。 */
  label: string;
  /** 次要说明，如日期、来源。 */
  meta?: string;
  /** 右侧标签，如「快照」。 */
  badge?: string;
  /** 是否为「默认项」（如「无（原版）」），用强调色区分。 */
  accent?: boolean;
}

/** 一次最多渲染多少项。 */
const MAX_VISIBLE = 60;

/**
 * 命令面板式的选择器。
 *
 * 为什么不用 `<select>` 或长列表：Fabric 对一个游戏版本能列出上百个 loader 版本，
 * 原版清单有九百多个版本。下拉框里翻几十项既慢又难找，而「输入两个字就剩下几项」
 * 是这类选择最快的交互——所以这里统一成搜索框 + 键盘导航。
 *
 * 键盘：↑↓ 移动、Enter 选中、Esc 关闭。鼠标点选同样有效。
 */
export function PickerDialog({
  title,
  placeholder,
  items,
  selectedId,
  emptyHint = "没有匹配项",
  busy = false,
  error = null,
  onRetry,
  onPick,
  onClose,
}: {
  title: string;
  placeholder: string;
  items: PickerItem[];
  selectedId: string | null;
  emptyHint?: string;
  /** 正在拉取列表：此时「0 项」是没有意义的，要显示「正在读取」。 */
  busy?: boolean;
  /** 拉取失败的原因。**必须与「空列表」分开**：否则「连不上」会被读成「上游没有」。 */
  error?: string | null;
  /** 重新拉取。给了才显示「重试」。 */
  onRetry?: () => void;
  onPick: (id: string) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) {
      return items;
    }
    return items.filter((item) =>
      `${item.label} ${item.meta ?? ""}`.toLowerCase().includes(q)
    );
  }, [items, query]);

  const visible = filtered.slice(0, MAX_VISIBLE);
  const hidden = filtered.length - visible.length;

  // 高亮项滚进可视区：键盘连续按 ↓ 时不能跑到屏幕外。
  // 这是与 DOM 同步的副作用（不是 setState），所以用 effect 是对的。
  useEffect(() => {
    const container = listRef.current;
    const current = container?.querySelector<HTMLElement>(`[data-index="${active}"]`);
    // jsdom 没有实现 scrollIntoView，测试环境下跳过即可。
    current?.scrollIntoView?.({ block: "nearest" });
  }, [active]);

  function handleKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActive((index) => Math.min(index + 1, visible.length - 1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive((index) => Math.max(index - 1, 0));
    } else if (event.key === "Enter") {
      event.preventDefault();
      const item = visible[active];
      if (item) {
        onPick(item.id);
      }
    } else if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }

  return (
    <div className="absolute inset-0 z-50 flex items-start justify-center pt-[12vh]">
      <button
        type="button"
        aria-label="关闭"
        onClick={onClose}
        className="absolute inset-0 bg-black/40 backdrop-blur-[1px]"
      />

      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="relative flex max-h-[62vh] w-[560px] max-w-[92%] flex-col overflow-hidden rounded-2xl bg-[var(--color-surface-raised)] shadow-2xl ring-1 ring-[var(--color-border-strong)]"
      >
        <div className="flex items-center gap-2 border-b border-[var(--color-border-subtle)] px-4 py-3">
          <span className="text-[13px] font-medium text-[var(--color-text-secondary)]">
            {title}
          </span>
          <input
            autoFocus
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              // 查询变了就把高亮拉回第一项，否则会停在已不在列表里的位置。
              setActive(0);
            }}
            onKeyDown={handleKeyDown}
            placeholder={placeholder}
            spellCheck={false}
            className="ml-2 min-w-0 flex-1 bg-transparent text-[14px] text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
          />
          <span className="shrink-0 text-[11px] tabular-nums text-[var(--color-text-muted)]">
            {busy ? "读取中" : `${filtered.length} 项`}
          </span>
        </div>

        <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto p-1.5">
          {error !== null ? (
            <div className="px-3 py-6">
              <p className="text-[13px] leading-relaxed break-words text-[var(--color-danger)]">
                读取失败：{error}
              </p>
              {onRetry && (
                <button
                  type="button"
                  onClick={onRetry}
                  className="mt-3 rounded-lg px-3 py-1.5 text-[12px] text-[var(--color-accent-text)] ring-1 ring-inset ring-[var(--color-border-strong)] transition-colors hover:bg-[var(--color-surface-hover)]"
                >
                  重试
                </button>
              )}
            </div>
          ) : (
            visible.length === 0 && (
              <p className="px-3 py-6 text-[13px] text-[var(--color-text-muted)]">
                {busy ? "正在读取…" : emptyHint}
              </p>
            )
          )}

          {visible.map((item, index) => {
            const selected = item.id === selectedId;
            const highlighted = index === active;
            return (
              <button
                key={item.id}
                type="button"
                data-index={index}
                onMouseEnter={() => setActive(index)}
                onClick={() => onPick(item.id)}
                className={`flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left transition-colors ${
                  highlighted ? "bg-[var(--color-surface-hover)]" : ""
                }`}
              >
                <span
                  className={`truncate font-mono text-[13px] ${
                    selected || item.accent
                      ? "text-[var(--color-accent-text)]"
                      : "text-[var(--color-text-primary)]"
                  }`}
                >
                  {item.label}
                </span>

                {item.badge && (
                  <span className="shrink-0 rounded bg-[var(--color-surface-hover)] px-1.5 py-0.5 text-[11px] text-[var(--color-warning)]">
                    {item.badge}
                  </span>
                )}

                <span className="ml-auto flex shrink-0 items-center gap-2">
                  {item.meta && (
                    <span className="text-[11px] tabular-nums text-[var(--color-text-muted)]">
                      {item.meta}
                    </span>
                  )}
                  {selected && (
                    <span className="text-[11px] text-[var(--color-accent-text)]">已选</span>
                  )}
                </span>
              </button>
            );
          })}

          {hidden > 0 && (
            <p className="px-3 py-2 text-[11px] text-[var(--color-text-muted)]">
              还有 {hidden} 项未显示，继续输入以缩小范围
            </p>
          )}
        </div>

        <div className="flex shrink-0 items-center gap-3 border-t border-[var(--color-border-subtle)] px-4 py-2 text-[11px] text-[var(--color-text-muted)]">
          <span>↑↓ 选择</span>
          <span>Enter 确认</span>
          <span>Esc 关闭</span>
        </div>
      </div>
    </div>
  );
}
