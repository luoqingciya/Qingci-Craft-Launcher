import type { VersionSummary } from "../api";
import { formatDate, typeLabel } from "../lib/format";

/** 版本列表。点击选中，选中项决定「安装 / 启动」作用于哪个版本。 */
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
  return (
    <section className="flex min-h-0 flex-col overflow-hidden rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-surface-raised)]">
      <div className="flex items-center justify-between border-b border-[var(--color-border-subtle)] px-4 py-3">
        <h2 className="text-sm font-medium">可用版本</h2>
        <span className="text-xs text-[var(--color-text-secondary)]">
          共 {versions.length} 个
        </span>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        <ul className="divide-y divide-[var(--color-border-subtle)]">
          {versions.map((version) => {
            const active = version.id === selected;
            return (
              <li key={version.id}>
                <button
                  type="button"
                  disabled={disabled}
                  aria-pressed={active}
                  onClick={() => onSelect(version.id)}
                  className={`flex w-full items-center justify-between px-4 py-2.5 text-left text-sm disabled:cursor-not-allowed ${
                    active
                      ? "bg-[var(--color-accent-soft)] text-[var(--color-accent)]"
                      : "hover:bg-[var(--color-accent-soft)] disabled:opacity-50"
                  }`}
                >
                  <span className="font-mono">{version.id}</span>
                  <span className="flex items-center gap-3 text-xs text-[var(--color-text-secondary)]">
                    <span>{typeLabel(version.type)}</span>
                    <span>{formatDate(version.releaseTime)}</span>
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      </div>
    </section>
  );
}
