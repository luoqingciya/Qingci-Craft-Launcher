import { useEffect, useRef } from "react";

import { MAX_LINES, type LogEntry, type LogStreamKind } from "../lib/logs";

const STREAM_STYLES: Record<LogStreamKind, string> = {
  stdout: "text-[var(--color-text-primary)]",
  stderr: "text-red-400",
  system: "text-[var(--color-accent)]",
};

/** 日志面板。 */
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
    <section className="flex min-h-0 flex-col overflow-hidden rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-surface-raised)]">
      <div className="flex items-center justify-between border-b border-[var(--color-border-subtle)] px-4 py-3">
        <h2 className="text-sm font-medium">游戏日志</h2>
        <div className="flex items-center gap-3">
          <span className="text-xs text-[var(--color-text-secondary)]">
            {logs.length >= MAX_LINES ? `仅显示最近 ${MAX_LINES} 行` : `${logs.length} 行`}
          </span>
          <button
            type="button"
            onClick={onClear}
            disabled={logs.length === 0}
            className="rounded border border-[var(--color-border-subtle)] px-2 py-1 text-xs hover:bg-[var(--color-accent-soft)] disabled:opacity-40"
          >
            清空
          </button>
        </div>
      </div>

      <div ref={container} className="min-h-0 flex-1 overflow-y-auto px-4 py-2">
        {logs.length === 0 ? (
          <p className="text-xs text-[var(--color-text-secondary)]">
            启动游戏后日志会显示在这里。
          </p>
        ) : (
          <pre className="whitespace-pre-wrap break-all font-mono text-xs leading-relaxed">
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
