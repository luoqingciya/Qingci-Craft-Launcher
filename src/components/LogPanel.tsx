// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { useEffect, useMemo, useRef, useState } from "react";

import { copyText } from "../lib/clipboard";
import {
  MAX_LINES,
  filterLogs,
  levelOf,
  type LogEntry,
  type LogLevel,
  type LogStreamKind,
} from "../lib/logs";

const STREAM_STYLES: Record<LogStreamKind, string> = {
  stdout: "text-[var(--color-text-primary)]",
  stderr: "text-[var(--color-danger)]",
  system: "text-[var(--color-accent-text)]",
};

/** 级别筛选的选项。`all` 不是 `LogLevel`，所以单独列出来。 */
const FILTERS: { id: LogLevel | "all"; label: string }[] = [
  { id: "all", label: "全部" },
  { id: "info", label: "信息" },
  { id: "warn", label: "警告" },
  { id: "error", label: "错误" },
];

/** 判定「贴着底部」的容差（像素）。留一点余量，避免舍入误差让跟随意外断开。 */
const STICK_THRESHOLD = 40;

/**
 * 日志面板。
 *
 * 与版本列表一样不画外框，由布局决定边界。日志用等宽字体、行高收紧，
 * 让同一屏能看到更多行——排查问题时行数比字号重要。
 */
export function LogPanel({ logs, onClear }: { logs: LogEntry[]; onClear: () => void }) {
  const container = useRef<HTMLDivElement>(null);

  const [level, setLevel] = useState<LogLevel | "all">("all");
  const [query, setQuery] = useState("");
  const [copied, setCopied] = useState(false);

  const visible = useMemo(() => filterLogs(logs, level, query), [logs, level, query]);
  const filtered = visible.length !== logs.length;

  /**
   * 是否跟随到底部。
   *
   * 用「用户有没有滚开」这个状态判断，而不是每次量「离底部多远」：
   * 游戏启动时第一批日志是一次性灌进来的，此刻 `scrollTop` 还是 0 而内容已经
   * 超过一屏——按距离算就判定「用户不在底部」，于是永远不跟随。
   * 只有用户自己滚动时才更新这个标志，内容增长不会影响它。
   */
  const stick = useRef(true);

  useEffect(() => {
    const element = container.current;
    if (!element) {
      return;
    }

    // 清空后重新开始跟随。
    if (logs.length === 0) {
      stick.current = true;
      return;
    }

    // 一行都没筛出来时不滚动——那会把滚动条按到底，解除筛选后位置就丢了。
    if (stick.current && visible.length > 0) {
      element.scrollTop = element.scrollHeight;
    }
  }, [logs, visible]);

  function handleScroll() {
    const element = container.current;
    if (!element) {
      return;
    }
    stick.current =
      element.scrollHeight - element.scrollTop - element.clientHeight < STICK_THRESHOLD;
  }

  async function copyVisible() {
    // 只复制**看得见的**行：筛出错误再复制，是排查时最常见的用法。
    const ok = await copyText(visible.map((log) => log.text).join("\n"));
    if (ok) {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    }
  }

  return (
    <section className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-9 shrink-0 items-center justify-between gap-2 px-4">
        <h2 className="text-[12px] font-medium uppercase tracking-wider text-[var(--color-text-secondary)]">
          日志
        </h2>
        <div className="flex items-center gap-1">
          <span className="mr-1 text-[12px] tabular-nums text-[var(--color-text-muted)]">
            {filtered
              ? `${visible.length} / ${logs.length} 行`
              : logs.length >= MAX_LINES
                ? `最近 ${MAX_LINES} 行`
                : `${logs.length} 行`}
          </span>
          <ToolButton onClick={() => void copyVisible()} disabled={visible.length === 0}>
            {copied ? "已复制" : "复制"}
          </ToolButton>
          <ToolButton onClick={onClear} disabled={logs.length === 0}>
            清空
          </ToolButton>
        </div>
      </div>

      {/* 筛选条只在有日志时出现：空面板上摆一排点不动的按钮只是噪音。 */}
      {logs.length > 0 && (
        <div className="flex shrink-0 items-center gap-2 px-4 pb-2">
          <div role="group" aria-label="按级别筛选" className="flex items-center gap-0.5">
            {FILTERS.map((item) => {
              const active = level === item.id;
              return (
                <button
                  key={item.id}
                  type="button"
                  aria-pressed={active}
                  onClick={() => setLevel(item.id)}
                  className={`rounded-md px-2 py-0.5 text-[12px] transition-colors ${
                    active
                      ? "bg-[var(--color-accent-soft)] font-medium text-[var(--color-accent-text)]"
                      : "text-[var(--color-text-muted)] hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-text-secondary)]"
                  }`}
                >
                  {item.label}
                </button>
              );
            })}
          </div>
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="筛选日志…"
            spellCheck={false}
            aria-label="筛选日志"
            className="min-w-0 flex-1 rounded-md bg-[var(--color-surface-raised)] px-2 py-1 text-[12px] ring-1 ring-inset ring-[var(--color-border-subtle)] outline-none placeholder:text-[var(--color-text-muted)] focus:ring-[var(--color-accent)]"
          />
        </div>
      )}

      <div
        ref={container}
        role="log"
        aria-live="polite"
        aria-label="游戏日志"
        onScroll={handleScroll}
        className="mx-4 mb-4 min-h-0 flex-1 overflow-y-auto rounded-lg bg-[var(--color-log-surface)] px-3 py-2 ring-1 ring-inset ring-[var(--color-border-subtle)]"
      >
        {logs.length === 0 ? (
          <p className="text-[12px] text-[var(--color-text-muted)]">
            启动游戏后，输出会实时显示在这里。
          </p>
        ) : visible.length === 0 ? (
          <p className="text-[12px] text-[var(--color-text-muted)]">没有符合条件的日志。</p>
        ) : (
          <pre className="whitespace-pre-wrap break-all font-mono text-[12px] leading-[1.6]">
            {visible.map((log) => (
              <div
                key={log.id}
                className={STREAM_STYLES[log.stream]}
                // 警告行左侧加一道竖线，扫一眼就能定位，不必逐行读级别。
                style={
                  levelOf(log) === "warn"
                    ? { boxShadow: "inset 2px 0 0 var(--color-warning)" , paddingLeft: 6 }
                    : undefined
                }
              >
                {log.text}
              </div>
            ))}
          </pre>
        )}
      </div>
    </section>
  );
}

/** 标题栏上的小按钮。 */
function ToolButton({
  onClick,
  disabled,
  children,
}: {
  onClick: () => void;
  disabled: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="rounded px-1.5 py-0.5 text-[12px] text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-text-primary)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-accent)] disabled:opacity-30"
    >
      {children}
    </button>
  );
}
