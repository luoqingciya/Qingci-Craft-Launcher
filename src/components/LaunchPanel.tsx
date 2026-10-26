// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import type { GameStatus, InstallProgressEvent, LoaderOption, VersionSummary } from "../api";
import { InstallProgressBar } from "./SettingsPanel";
import { LoaderSelect } from "./LoaderSelect";

/**
 * 「启动」页的左栏：账户 + 当前版本 + 启动按钮。
 *
 * 把这三块放在同一栏，是因为它们回答的是同一个问题——「我现在点一下会启动什么」。
 * 版本列表放到独立标签页，避免左栏被几百个版本撑爆。
 */
export function LaunchPanel({
  playerName,
  selected,
  selectedVersion,
  loaders,
  loader,
  loaderBusy,
  busy,
  status,
  progress,
  message,
  error,
  canAct,
  onLoaderChange,
  onLaunch,
  onInstall,
  onStop,
  onBrowseVersions,
}: {
  playerName: string;
  selected: string | null;
  selectedVersion: VersionSummary | undefined;
  loaders: LoaderOption[];
  loader: string;
  loaderBusy: boolean;
  busy: boolean;
  status: GameStatus;
  progress: InstallProgressEvent | null;
  message: string | null;
  error: string | null;
  canAct: boolean;
  onLoaderChange: (value: string) => void;
  onLaunch: () => void;
  onInstall: () => void;
  onStop: () => void;
  onBrowseVersions: () => void;
}) {
  const running = status.running;
  const initial = playerName.trim().charAt(0) || "?";

  return (
    <div className="flex h-full flex-col gap-4">
      {/* 账户 */}
      <section className="rounded-2xl bg-[var(--color-surface-raised)] p-4 ring-1 ring-inset ring-[var(--color-border-subtle)]">
        <div className="flex items-center gap-3">
          <span className="flex h-14 w-14 items-center justify-center rounded-xl bg-gradient-to-br from-[#14b8a6] to-[var(--color-accent)] text-[22px] font-semibold text-white shadow-[0_8px_20px_-8px_rgba(20,184,166,0.8)]">
            {initial}
          </span>
          <div className="min-w-0">
            <div className="truncate text-[15px] font-medium">{playerName}</div>
            <div className="mt-0.5 flex items-center gap-1.5 text-[12px] text-[var(--color-text-secondary)]">
              <span className="h-1.5 w-1.5 rounded-full bg-[var(--color-warning)]" />
              离线账户
            </div>
          </div>
        </div>
      </section>

      {/* 当前版本 */}
      <section className="rounded-2xl bg-[var(--color-surface-raised)] p-4 ring-1 ring-inset ring-[var(--color-border-subtle)]">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="text-[12px] font-medium uppercase tracking-wider text-[var(--color-text-muted)]">
            当前版本
          </h2>
          <button
            type="button"
            onClick={onBrowseVersions}
            className="rounded-md px-1.5 py-0.5 text-[12px] text-[var(--color-accent-text)] hover:bg-[var(--color-accent-soft)]"
          >
            选择…
          </button>
        </div>

        <div className="truncate font-mono text-[17px] text-[var(--color-text-primary)]">
          {selected ?? "未选择"}
        </div>
        {selectedVersion && (
          <div className="mt-1 text-[12px] text-[var(--color-text-muted)]">
            {selectedVersion.type === "release" ? "正式版" : "快照"} ·{" "}
            {selectedVersion.releaseTime.slice(0, 10)}
          </div>
        )}

        <div className="mt-3">
          <LoaderSelect
            loaders={loaders}
            value={loader}
            busy={loaderBusy}
            disabled={!canAct || running || !selected}
            onChange={onLoaderChange}
          />
        </div>
      </section>

      <div className="mt-auto flex flex-col gap-2">
        {/* 没选版本时不要给一个点不动的死按钮：直接变成「选择版本」的入口。 */}
        <button
          type="button"
          onClick={selected ? onLaunch : onBrowseVersions}
          disabled={selected ? !canAct || running : false}
          className="group flex h-16 flex-col items-center justify-center gap-0.5 rounded-xl bg-[var(--color-accent)] text-white shadow-[0_10px_24px_-10px_var(--color-accent-shadow)] transition-all hover:bg-[var(--color-accent-strong)] active:translate-y-px disabled:bg-[var(--color-surface-raised)] disabled:text-[var(--color-text-muted)] disabled:shadow-none disabled:ring-1 disabled:ring-inset disabled:ring-[var(--color-border-subtle)]"
        >
          <span className="text-[16px] font-semibold">
            {selected ? (running ? "游戏运行中" : "启动游戏") : "选择版本"}
          </span>
          <span className="text-[11px] opacity-80">
            {selected
              ? loader
                ? `${selected} · Fabric ${loader}`
                : selected
              : "还没有选择版本"}
          </span>
        </button>

        <div className="flex gap-2">
          <button
            type="button"
            onClick={onInstall}
            disabled={!canAct || running}
            className="h-10 flex-1 rounded-xl text-[13px] text-[var(--color-text-secondary)] ring-1 ring-inset ring-[var(--color-border-strong)] transition-colors hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-text-primary)] disabled:opacity-40"
          >
            {busy ? "处理中…" : "安装 / 检查文件"}
          </button>
          <button
            type="button"
            onClick={onStop}
            disabled={!running}
            className="h-10 rounded-xl px-4 text-[13px] text-[var(--color-danger)] ring-1 ring-inset ring-[var(--color-danger)]/40 transition-colors hover:bg-[var(--color-danger)]/10 disabled:opacity-30"
          >
            停止
          </button>
        </div>

        {(progress || message || error) && (
          <div className="flex flex-col gap-2 rounded-xl bg-[var(--color-surface-raised)] p-3 ring-1 ring-inset ring-[var(--color-border-subtle)]">
            {progress && <InstallProgressBar progress={progress} />}
            {message && (
              <p className="text-[12px] leading-relaxed text-[var(--color-success)]">{message}</p>
            )}
            {error && (
              <p className="text-[12px] leading-relaxed text-[var(--color-danger)]">{error}</p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
