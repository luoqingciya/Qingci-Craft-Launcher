import type { InstallProgressEvent, SettingsView } from "../api";
import { phaseLabel } from "../lib/format";

/** 游戏目录设置。目录变更会影响后续所有下载与启动，因此独立成块。 */
export function GameDirectory({
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

/** 安装进度条。阶段切换时文字随之变化，避免把两个阶段的百分比混为一谈。 */
export function InstallProgressBar({ progress }: { progress: InstallProgressEvent }) {
  return (
    <div className="mt-2">
      <div className="flex items-center justify-between text-xs text-[var(--color-text-secondary)]">
        <span>
          正在下载{phaseLabel(progress.phase)} {progress.completed}/{progress.total}
        </span>
        <span>{progress.percent}%</span>
      </div>
      <div className="mt-1 h-1.5 overflow-hidden rounded bg-[var(--color-border-subtle)]">
        <div
          className="h-full bg-[var(--color-accent)] transition-[width] duration-200"
          style={{ width: `${progress.percent}%` }}
        />
      </div>
    </div>
  );
}
