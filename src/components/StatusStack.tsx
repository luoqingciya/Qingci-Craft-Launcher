// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import type { InstallProgressEvent } from "../api";
import { phaseLabel } from "../lib/format";
import { ExternalLink } from "./ExternalLink";

/**
 * 浮在页面上的状态提示（下载进度 / 成功 / 错误）。
 *
 * 为什么不做成页面里的一块：下载进度是有则无、无则有的东西，塞进左栏会把
 * 「启动游戏」按钮往上顶——正要点按钮的时候按钮跑掉，是这套界面里最烦人的
 * 一处。浮层不参与布局，进出都不影响任何组件的位置。
 *
 * 去留规则由调用方决定（这里只负责画）：进度跟着 `progress`，成功的自动过期
 * 在 App 里排的定时器，错误一直留到用户关掉。
 */
export function StatusStack({
  progress,
  message,
  error,
  mirrored,
  onDismissMessage,
  onDismissError,
}: {
  progress: InstallProgressEvent | null;
  message: string | null;
  error: string | null;
  /** 下载是否走 BMCLAPI 镜像；走的话必须标注来源（使用协议要求）。 */
  mirrored: boolean;
  onDismissMessage: () => void;
  onDismissError: () => void;
}) {
  if (!progress && !message && !error) {
    return null;
  }

  return (
    <div
      // 固定定位、不参与文档流：这是「不挤占页面组件」的实现方式。
      className="pointer-events-none fixed bottom-4 right-4 z-40 flex w-[340px] flex-col gap-2"
      aria-live="polite"
    >
      {progress && (
        <div className="status-enter pointer-events-auto rounded-xl border border-[var(--color-border-subtle)] bg-[var(--color-surface-raised)] p-3 shadow-[0_16px_40px_-16px_rgba(0,0,0,0.45)]">
          <div className="flex items-baseline justify-between gap-2">
            <span className="text-[12px] text-[var(--color-text-secondary)]">
              正在下载{phaseLabel(progress.phase)} {progress.completed}/{progress.total}
            </span>
            <span className="font-mono text-[12px] tabular-nums text-[var(--color-text-primary)]">
              {progress.percent}%
            </span>
          </div>
          <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-[var(--color-border-subtle)]">
            <div
              className="h-full rounded-full bg-[var(--color-accent)] transition-[width] duration-300 ease-out"
              style={{ width: `${Math.min(100, Math.max(0, progress.percent))}%` }}
            />
          </div>
          {mirrored && (
            <p className="mt-2 text-[11px] text-[var(--color-text-muted)]">
              下载源：
              <ExternalLink href="https://bmclapidoc.bangbang93.com/">BMCLAPI</ExternalLink>
              镜像
            </p>
          )}
        </div>
      )}

      {message && (
        <StatusCard tone="success" onDismiss={onDismissMessage}>
          {message}
        </StatusCard>
      )}

      {error && (
        <StatusCard tone="danger" onDismiss={onDismissError}>
          {error}
        </StatusCard>
      )}
    </div>
  );
}

function StatusCard({
  tone,
  onDismiss,
  children,
}: {
  tone: "success" | "danger";
  onDismiss: () => void;
  children: React.ReactNode;
}) {
  const color = tone === "success" ? "var(--color-success)" : "var(--color-danger)";
  return (
    <div
      className="status-enter pointer-events-auto flex items-start gap-2.5 rounded-xl border border-[var(--color-border-subtle)] bg-[var(--color-surface-raised)] p-3 shadow-[0_16px_40px_-16px_rgba(0,0,0,0.45)]"
      role={tone === "danger" ? "alert" : "status"}
    >
      <span
        aria-hidden
        className="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full"
        style={{ backgroundColor: color }}
      >
        <svg viewBox="0 0 12 12" className="h-2.5 w-2.5" fill="none" stroke="#0b1220" strokeWidth="2">
          {tone === "success" ? (
            <path d="M2.5 6.2 4.8 8.5 9.5 3.8" strokeLinecap="round" strokeLinejoin="round" />
          ) : (
            <path d="M3 3l6 6M9 3l-6 6" strokeLinecap="round" />
          )}
        </svg>
      </span>
      <p className="min-w-0 flex-1 text-[12px] leading-relaxed text-[var(--color-text-primary)]">
        {children}
      </p>
      <button
        type="button"
        onClick={onDismiss}
        aria-label="关闭提示"
        className="-mr-1 -mt-1 shrink-0 rounded p-1 text-[var(--color-text-muted)] transition-colors hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-text-primary)]"
      >
        <svg viewBox="0 0 12 12" className="h-3 w-3" fill="none" stroke="currentColor" strokeWidth="1.6">
          <path d="M3 3l6 6M9 3l-6 6" strokeLinecap="round" />
        </svg>
      </button>
    </div>
  );
}
