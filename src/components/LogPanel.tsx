// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { useEffect, useRef } from "react";

import { MAX_LINES, type LogEntry, type LogStreamKind } from "../lib/logs";

const STREAM_STYLES: Record<LogStreamKind, string> = {
  stdout: "text-[var(--color-text-primary)]",
  stderr: "text-[var(--color-danger)]",
  system: "text-[var(--color-accent-text)]",
};

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

    if (stick.current) {
      element.scrollTop = element.scrollHeight;
    }
  }, [logs]);

  function handleScroll() {
    const element = container.current;
    if (!element) {
      return;
    }
    stick.current =
      element.scrollHeight - element.scrollTop - element.clientHeight < STICK_THRESHOLD;
  }

  return (
    <section className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-9 shrink-0 items-center justify-between px-4">
        <h2 className="text-[12px] font-medium uppercase tracking-wider text-[var(--color-text-secondary)]">
          日志
        </h2>
        <div className="flex items-center gap-3">
          <span className="text-[12px] tabular-nums text-[var(--color-text-muted)]">
            {logs.length >= MAX_LINES ? `最近 ${MAX_LINES} 行` : `${logs.length} 行`}
          </span>
          <button
            type="button"
            onClick={onClear}
            disabled={logs.length === 0}
            className="rounded px-1.5 py-0.5 text-[12px] text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-text-primary)] disabled:opacity-30"
          >
            清空
          </button>
        </div>
      </div>

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
        ) : (
          <pre className="whitespace-pre-wrap break-all font-mono text-[12px] leading-[1.6]">
            {logs.map((log) => (
              <div key={log.id} className={STREAM_STYLES[log.stream]}>
                {log.text}
              </div>
            ))}
          </pre>
        )}
      </div>
    </section>
  );
}
