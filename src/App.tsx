import { useEffect, useState } from "react";

import { listVersions, type VersionSummary } from "./api";

/** 加载状态机。用联合类型而不是多个布尔，避免出现「既在加载又有错误」这种非法状态。 */
type LoadState =
  | { status: "loading" }
  | { status: "ready"; versions: VersionSummary[] }
  | { status: "failed"; message: string };

const TYPE_LABELS: Record<string, string> = {
  release: "正式版",
  snapshot: "快照",
  old_beta: "Beta",
  old_alpha: "Alpha",
  other: "其他",
};

function formatDate(iso: string): string {
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

export default function App() {
  const [state, setState] = useState<LoadState>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;

    listVersions()
      .then((versions) => {
        // 组件卸载后不再写状态，避免 React 警告与竞态。
        if (!cancelled) {
          setState({ status: "ready", versions });
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setState({ status: "failed", message: String(error) });
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <main className="flex h-full flex-col gap-4 p-6">
      <header className="flex items-baseline justify-between">
        <h1 className="text-lg font-semibold tracking-tight">Qingci Craft Launcher</h1>
        <span className="text-xs text-[var(--color-text-secondary)]">Phase 2 · 界面骨架</span>
      </header>

      <section className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-surface-raised)]">
        <div className="flex items-center justify-between border-b border-[var(--color-border-subtle)] px-4 py-3">
          <h2 className="text-sm font-medium">可用版本</h2>
          {state.status === "ready" && (
            <span className="text-xs text-[var(--color-text-secondary)]">
              共 {state.versions.length} 个
            </span>
          )}
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto">
          {state.status === "loading" && (
            <p className="p-4 text-sm text-[var(--color-text-secondary)]">正在获取版本清单…</p>
          )}

          {state.status === "failed" && (
            <p className="p-4 text-sm text-red-400">获取版本清单失败：{state.message}</p>
          )}

          {state.status === "ready" && (
            <ul className="divide-y divide-[var(--color-border-subtle)]">
              {state.versions.map((version) => (
                <li
                  key={version.id}
                  className="flex items-center justify-between px-4 py-2.5 text-sm hover:bg-[var(--color-accent-soft)]"
                >
                  <span className="font-mono">{version.id}</span>
                  <span className="flex items-center gap-3 text-xs text-[var(--color-text-secondary)]">
                    <span>{TYPE_LABELS[version.type] ?? version.type}</span>
                    <span>{formatDate(version.releaseTime)}</span>
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
      </section>

      <footer className="text-xs text-[var(--color-text-secondary)]">
        当前仅打通「版本清单」这一条链路，安装与启动将在后续接入。
      </footer>
    </main>
  );
}
