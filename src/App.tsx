import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import {
  getSettings,
  listVersions,
  setGameDir,
  type SettingsView,
  type VersionSummary,
} from "./api";

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

/** 游戏目录设置。目录变更会影响后续所有下载与启动，因此独立成块。 */
function GameDirectory({
  settings,
  busy,
  onChange,
  onReset,
}: {
  settings: SettingsView | null;
  busy: boolean;
  onChange: () => void;
  onReset: () => void;
}) {
  return (
    <section className="rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-surface-raised)] px-4 py-3">
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h2 className="text-sm font-medium">游戏目录</h2>
            {settings?.isCustom && (
              <span className="rounded bg-[var(--color-accent-soft)] px-1.5 py-0.5 text-[10px] text-[var(--color-accent)]">
                自定义
              </span>
            )}
          </div>
          <p
            className="mt-1 truncate font-mono text-xs text-[var(--color-text-secondary)]"
            title={settings?.gameDir ?? ""}
          >
            {settings?.gameDir ?? "读取中…"}
          </p>
        </div>

        <div className="flex shrink-0 gap-2">
          <button
            type="button"
            onClick={onChange}
            disabled={busy || settings === null}
            className="rounded border border-[var(--color-border-subtle)] px-3 py-1.5 text-xs hover:bg-[var(--color-accent-soft)] disabled:opacity-40"
          >
            更改…
          </button>
          <button
            type="button"
            onClick={onReset}
            disabled={busy || settings === null || !settings.isCustom}
            className="rounded border border-[var(--color-border-subtle)] px-3 py-1.5 text-xs hover:bg-[var(--color-accent-soft)] disabled:opacity-40"
            title={settings ? `恢复为 ${settings.defaultGameDir}` : undefined}
          >
            恢复默认
          </button>
        </div>
      </div>

      {settings?.fallbackReason && (
        <p className="mt-2 text-xs text-amber-400">⚠ {settings.fallbackReason}</p>
      )}
    </section>
  );
}

export default function App() {
  const [state, setState] = useState<LoadState>({ status: "loading" });
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

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

    getSettings()
      .then((view) => {
        if (!cancelled) {
          setSettings(view);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setSettingsError(String(error));
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const applyGameDir = useCallback(async (path: string | null) => {
    setBusy(true);
    setSettingsError(null);
    try {
      setSettings(await setGameDir(path));
    } catch (error: unknown) {
      setSettingsError(String(error));
    } finally {
      setBusy(false);
    }
  }, []);

  const pickDirectory = useCallback(async () => {
    // 对话框返回 `string | string[] | null`：取消时为 null，多选时是数组。
    const picked = await open({
      directory: true,
      multiple: false,
      title: "选择游戏目录",
    });
    if (typeof picked === "string") {
      await applyGameDir(picked);
    }
  }, [applyGameDir]);

  return (
    <main className="flex h-full flex-col gap-4 p-6">
      <header className="flex items-baseline justify-between">
        <h1 className="text-lg font-semibold tracking-tight">Qingci Craft Launcher</h1>
        <span className="text-xs text-[var(--color-text-secondary)]">Phase 2 · 界面骨架</span>
      </header>

      <GameDirectory
        settings={settings}
        busy={busy}
        onChange={pickDirectory}
        onReset={() => void applyGameDir(null)}
      />

      {settingsError && (
        <p className="rounded border border-red-500/40 bg-red-500/10 px-4 py-2 text-xs text-red-400">
          游戏目录设置失败：{settingsError}
        </p>
      )}

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
        当前已打通「版本清单」与「游戏目录设置」，安装与启动将在后续接入。
      </footer>
    </main>
  );
}
