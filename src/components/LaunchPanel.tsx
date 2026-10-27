// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import type { Account, GameStatus, InstallProgressEvent, VersionSummary } from "../api";
import { InstallProgressBar } from "./SettingsPanel";

/**
 * 「启动」页的左栏：账户 + 当前版本 + 启动按钮。
 *
 * 把这三块放在同一栏，是因为它们回答的是同一个问题——「我现在点一下会启动什么」。
 *
 * 版本与加载器都用「点击打开选择器」而不是下拉框：原版清单九百多项、
 * 加载器上百项，下拉框里翻不动。选择器带搜索与键盘导航（见 PickerDialog）。
 */
export function LaunchPanel({
  playerName,
  account,
  msaLogin,
  selected,
  selectedVersion,
  loaderLabel,
  busy,
  status,
  progress,
  message,
  error,
  canAct,
  onPickVersion,
  onPickLoader,
  onLaunch,
  onInstall,
  onStop,
  onLogin,
  onSignOut,
}: {
  /** 离线玩家名。已登录时界面显示的是账号名，这里只是回落值。 */
  playerName: string;
  /** 已登录的账号，未登录为 `null`。 */
  account: Account | null;
  /** 本次构建是否内置了微软登录能力。 */
  msaLogin: boolean;
  selected: string | null;
  selectedVersion: VersionSummary | undefined;
  /** 当前加载器的显示文案，如「无（原版）」或「Fabric 0.20.1」。 */
  loaderLabel: string;
  busy: boolean;
  status: GameStatus;
  progress: InstallProgressEvent | null;
  message: string | null;
  error: string | null;
  canAct: boolean;
  onPickVersion: () => void;
  onPickLoader: () => void;
  onLaunch: () => void;
  onInstall: () => void;
  onStop: () => void;
  onLogin: () => void;
  onSignOut: () => void;
}) {
  const running = status.running;
  const displayName = account?.name ?? playerName;
  const initial = displayName.trim().charAt(0) || "?";

  return (
    <div className="flex h-full flex-col gap-4">
      {/* 账户 */}
      <section className="rounded-2xl bg-[var(--color-surface-raised)] p-4 ring-1 ring-inset ring-[var(--color-border-subtle)]">
        <div className="flex items-center gap-3">
          <span className="flex h-14 w-14 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-[#22a7c4] to-[var(--color-accent)] text-[22px] font-semibold text-white shadow-[0_8px_20px_-8px_var(--color-accent-shadow)]">
            {initial}
          </span>
          <div className="min-w-0 flex-1">
            <div className="truncate text-[15px] font-medium">{displayName}</div>
            <div className="mt-0.5 flex items-center gap-1.5 text-[12px] text-[var(--color-text-secondary)]">
              <span
                className={`h-1.5 w-1.5 rounded-full ${
                  account ? "bg-[var(--color-success)]" : "bg-[var(--color-warning)]"
                }`}
              />
              {account ? "正版账户" : "离线账户"}
            </div>
          </div>
          {account ? (
            <button
              type="button"
              onClick={onSignOut}
              disabled={running}
              className="shrink-0 rounded-lg px-2.5 py-1 text-[12px] text-[var(--color-text-secondary)] ring-1 ring-inset ring-[var(--color-border-strong)] transition-colors hover:bg-[var(--color-surface-hover)] disabled:opacity-50"
            >
              退出
            </button>
          ) : (
            msaLogin && (
              <button
                type="button"
                onClick={onLogin}
                disabled={running}
                className="shrink-0 rounded-lg px-2.5 py-1 text-[12px] text-[var(--color-accent-text)] ring-1 ring-inset ring-[var(--color-border-strong)] transition-colors hover:bg-[var(--color-surface-hover)] disabled:opacity-50"
              >
                登录
              </button>
            )
          )}
        </div>
      </section>

      {/* 当前版本 + 加载器：两行都是「可点击的选择入口」 */}
      <section className="overflow-hidden rounded-2xl bg-[var(--color-surface-raised)] ring-1 ring-inset ring-[var(--color-border-subtle)]">
        <button
          type="button"
          onClick={onPickVersion}
          disabled={running}
          aria-label="更换版本"
          className="flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-[var(--color-surface-hover)] disabled:opacity-60"
        >
          <div className="min-w-0 flex-1">
            <div className="text-[12px] text-[var(--color-text-muted)]">版本</div>
            <div className="mt-0.5 truncate font-mono text-[16px] text-[var(--color-text-primary)]">
              {selected ?? "未选择"}
            </div>
            {selectedVersion && (
              <div className="mt-0.5 text-[11px] text-[var(--color-text-muted)]">
                {selectedVersion.type === "release" ? "正式版" : "快照"} ·{" "}
                {selectedVersion.releaseTime.slice(0, 10)}
              </div>
            )}
          </div>
          <span className="shrink-0 text-[12px] text-[var(--color-accent-text)]">更换</span>
        </button>

        <div className="h-px bg-[var(--color-border-subtle)]" />

        <button
          type="button"
          onClick={onPickLoader}
          disabled={running || !selected}
          aria-label="更换加载器"
          className="flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-[var(--color-surface-hover)] disabled:opacity-60"
        >
          <div className="min-w-0 flex-1">
            <div className="text-[12px] text-[var(--color-text-muted)]">加载器</div>
            <div className="mt-0.5 truncate text-[14px] text-[var(--color-text-primary)]">
              {selected ? loaderLabel : "先选择版本"}
            </div>
          </div>
          <span className="shrink-0 text-[12px] text-[var(--color-accent-text)]">更换</span>
        </button>
      </section>

      <div className="mt-auto flex flex-col gap-2">
        {/* 没选版本时不要给一个点不动的死按钮：直接变成「选择版本」的入口。 */}
        <button
          type="button"
          onClick={selected ? onLaunch : onPickVersion}
          disabled={selected ? !canAct || running : false}
          className="flex h-16 flex-col items-center justify-center gap-0.5 rounded-xl bg-[var(--color-accent)] text-white shadow-[0_10px_24px_-10px_var(--color-accent-shadow)] transition-all hover:bg-[var(--color-accent-strong)] active:translate-y-px disabled:bg-[var(--color-surface-raised)] disabled:text-[var(--color-text-muted)] disabled:shadow-none disabled:ring-1 disabled:ring-inset disabled:ring-[var(--color-border-subtle)]"
        >
          <span className="text-[16px] font-semibold">
            {selected ? (running ? "游戏运行中" : "启动游戏") : "选择版本"}
          </span>
          <span className="text-[11px] opacity-80">
            {selected
              ? loaderLabel === "无（原版）"
                ? selected
                : `${selected} · ${loaderLabel}`
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
