// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import type { Account, GameStatus, VersionSummary } from "../api";
import type { InstanceView } from "../api";

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
  instances,
  activeInstance,
  selected,
  selectedVersion,
  loaderLabel,
  busy,
  status,
  canAct,
  onPickVersion,
  onPickLoader,
  onLaunch,
  onInstall,
  onStop,
  onLogin,
  onSignOut,
  onSwitchInstance,
}: {
  /** 离线玩家名。已登录时界面显示的是账号名，这里只是回落值。 */  playerName: string;
  /** 已登录的账号，未登录为 `null`。 */
  account: Account | null;
  /** 本次构建是否内置了微软登录能力。 */
  msaLogin: boolean;
  /** 已知实例，用于切换器。 */
  instances: InstanceView[];
  /** 当前实例名。 */
  activeInstance: string;
  selected: string | null;
  selectedVersion: VersionSummary | undefined;
  /** 当前加载器的显示文案，如「无（原版）」或「Fabric 0.20.1」。 */
  loaderLabel: string;
  busy: boolean;
  status: GameStatus;
  canAct: boolean;
  onPickVersion: () => void;
  onPickLoader: () => void;
  onLaunch: () => void;
  onInstall: () => void;
  onStop: () => void;
  onLogin: () => void;
  onSignOut: () => void;
  /** 切换实例：整套游戏目录跟着换。 */
  onSwitchInstance: (name: string) => void;
}) {
  const running = status.running;
  const displayName = account?.name ?? playerName;
  const initial = displayName.trim().charAt(0) || "?";

  return (
    <div className="flex h-full flex-col gap-4">
      {/*
        一张卡装下「账户 → 实例 → 版本 → 加载器」四行，用发丝线分隔。
        原来是两张带间隙的卡片，视觉上是「一摞盒子」；合成一张之后层级更清楚，
        而且四行都属于「这台机器上玩什么」这件事，本来就不该分开。
      */}
      <section className="overflow-hidden rounded-2xl bg-[var(--color-surface-raised)] ring-1 ring-inset ring-[var(--color-border-subtle)]">
        {/* 账户 */}
        <div className="flex items-center gap-3 p-4">
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

        <div className="h-px bg-[var(--color-border-subtle)]" />

        {/* 当前版本 + 加载器：两行都是「可点击的选择入口」 */}
        {/* 只有一个实例时不显示切换器：多一行没得选的东西只是噪音。 */}
        {instances.length > 1 && (
          <>
            <label className="flex items-center gap-3 px-4 py-3">
              <span className="min-w-0 flex-1 text-[12px] text-[var(--color-text-muted)]">
                实例
              </span>
              {/* 原生 select 的下拉箭头在各平台长得都不一样，且与这套圆角对不上，
                  所以关掉原生外观自己画一个。 */}
              <span className="relative">
                <select
                  value={activeInstance}
                  onChange={(event) => onSwitchInstance(event.target.value)}
                  disabled={running}
                  aria-label="切换实例"
                  className="max-w-[180px] appearance-none rounded-lg bg-[var(--color-surface)] py-1.5 pl-2.5 pr-7 text-[13px] text-[var(--color-text-primary)] ring-1 ring-inset ring-[var(--color-border-subtle)] transition-colors hover:ring-[var(--color-border-strong)] focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-accent)] disabled:opacity-60"
                >
                  {instances.map((instance) => (
                    <option key={instance.name} value={instance.name}>
                      {instance.name}
                    </option>
                  ))}
                </select>
                <Chevron className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-[var(--color-text-muted)]" />
              </span>
            </label>
            <div className="h-px bg-[var(--color-border-subtle)]" />
          </>
        )}
        <button
          type="button"
          onClick={onPickVersion}
          disabled={running}
          aria-label="更换版本"
          className="group flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-[var(--color-surface-hover)] disabled:opacity-60"
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
          <Chevron className="shrink-0 text-[var(--color-text-muted)] transition-transform duration-150 group-hover:translate-x-0.5 group-hover:text-[var(--color-accent-text)]" />
        </button>

        <div className="h-px bg-[var(--color-border-subtle)]" />

        <button
          type="button"
          onClick={onPickLoader}
          disabled={running || !selected}
          aria-label="更换加载器"
          className="group flex w-full items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-[var(--color-surface-hover)] disabled:opacity-60"
        >
          <div className="min-w-0 flex-1">
            <div className="text-[12px] text-[var(--color-text-muted)]">加载器</div>
            <div className="mt-0.5 truncate text-[14px] text-[var(--color-text-primary)]">
              {selected ? loaderLabel : "先选择版本"}
            </div>
          </div>
          <Chevron className="shrink-0 text-[var(--color-text-muted)] transition-transform duration-150 group-hover:translate-x-0.5 group-hover:text-[var(--color-accent-text)]" />
        </button>
      </section>

      <div className="mt-auto flex flex-col gap-2">
        {/* 没选版本时不要给一个点不动的死按钮：直接变成「选择版本」的入口。 */}
        <button
          type="button"
          onClick={selected ? onLaunch : onPickVersion}
          disabled={selected ? !canAct || running : false}
          title={
            running ? "游戏正在运行" : busy ? "正在处理，请稍候" : selected ? undefined : "先选择一个版本"
          }
          className="flex h-16 flex-col items-center justify-center gap-0.5 rounded-xl bg-[var(--color-accent)] text-white shadow-[0_10px_24px_-10px_var(--color-accent-shadow)] transition-all hover:bg-[var(--color-accent-strong)] hover:shadow-[0_14px_28px_-12px_var(--color-accent-shadow)] active:translate-y-px focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-accent)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--color-surface)] disabled:bg-[var(--color-surface-raised)] disabled:text-[var(--color-text-muted)] disabled:shadow-none disabled:ring-1 disabled:ring-inset disabled:ring-[var(--color-border-subtle)]"
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
            className="h-10 flex-1 rounded-xl text-[13px] text-[var(--color-text-secondary)] ring-1 ring-inset ring-[var(--color-border-strong)] transition-colors hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-text-primary)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-accent)] disabled:opacity-40"
          >
            {busy ? "处理中…" : "安装 / 检查文件"}
          </button>
          <button
            type="button"
            onClick={onStop}
            disabled={!running}
            className="h-10 rounded-xl px-4 text-[13px] text-[var(--color-danger)] ring-1 ring-inset ring-[var(--color-danger)]/40 transition-colors hover:bg-[var(--color-danger)]/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-danger)] disabled:opacity-30"
          >
            停止
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * 右向箭头：可点击行的通用暗示。
 *
 * 用它替代原来那两个字「更换」——整行本来就可点，文字显得啰嗦，
 * 而箭头在悬停时轻微右移，比静态文字更能说明「点这里会展开」。
 */
function Chevron({ className = "" }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 16 16"
      aria-hidden
      className={`h-3.5 w-3.5 ${className}`}
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
    >
      <path d="M6 3.5 10.5 8 6 12.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
