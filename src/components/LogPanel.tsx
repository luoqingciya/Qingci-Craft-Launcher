// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { useEffect, useRef } from "react";

import { MAX_LINES, type LogEntry, type LogStreamKind } from "../lib/logs";

const STREAM_STYLES: Record<LogStreamKind, string> = {
  stdout: "text-[var(--color-text-primary)]",
  stderr: "text-[var(--color-danger)]",
  system: "text-[var(--color-accent)]",
};

/**
 * 日志面板。
 *
 * 与版本列表一样不画外框，由布局决定边界。日志用等宽字体、行高收紧，
 * 让同一屏能看到更多行——排查问题时行数比字号重要。
 */
export function LogPanel({ logs, onClear }: { logs: LogEntry[]; onClear: () => void }) {
  const container = useRef<HTMLDivElement>(null);

  // 新日志到达时滚到底部。只在「本来就在底部」时自动滚，
  // 否则用户向上翻看历史时会被不断拉回去。
  useEffect(() => {
    const element = container.current;
    if (!element || logs.length === 0) {
      return;
    }
    const nearBottom = element.scrollHeight - element.scrollTop - element.clientHeight < 80;
    if (nearBottom) {
      element.scrollTop = element.scrollHeight;
    }
  }, [logs]);

  return (
    <section className="flex min-h-0 flex-1 flex-col">
      <div className="flex h-9 shrink-0 items-center justify-between px-4">
        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[var(--color-text-secondary)]">
          日志
        </h2>
        <div className="flex items-center gap-3">
          <span className="text-[10px] tabular-nums text-[var(--color-text-muted)]">
            {logs.length >= MAX_LINES ? `最近 ${MAX_LINES} 行` : `${logs.length} 行`}
          </span>
          <button
            type="button"
            onClick={onClear}
            disabled={logs.length === 0}
            className="rounded px-1.5 py-0.5 text-[11px] text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-text-primary)] disabled:opacity-30"
          >
            清空
          </button>
        </div>
      </div>

      <div
        ref={container}
        className="mx-4 mb-4 min-h-0 flex-1 overflow-y-auto rounded-lg bg-[var(--color-log-surface)] px-3 py-2 ring-1 ring-inset ring-[var(--color-border-subtle)]"
      >
        {logs.length === 0 ? (
          <p className="text-[11px] text-[var(--color-text-muted)]">
            启动游戏后，输出会实时显示在这里。
          </p>
        ) : (
          <pre className="whitespace-pre-wrap break-all font-mono text-[11px] leading-[1.55]">
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
