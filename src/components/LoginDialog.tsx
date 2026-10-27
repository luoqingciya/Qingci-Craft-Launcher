// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import type { LoginPhase } from "../lib/useAccount";
import { ExternalLink } from "./ExternalLink";

/**
 * 设备码登录对话框。
 *
 * 用户在浏览器里输码完成登录，启动器这边只是等——所以这里最重要的一件事是
 * **把码和网址显示得足够清楚**，让人不用来回切窗口找。轮询由后端负责，
 * 界面上没有「检查一下」这种按钮：那会把协议细节漏到界面里。
 */
export function LoginDialog({ phase, onCancel }: { phase: LoginPhase; onCancel: () => void }) {
  if (phase.kind === "idle") {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6">
      <div
        role="dialog"
        aria-modal="true"
        aria-label="登录微软账号"
        className="w-full max-w-[420px] rounded-2xl bg-[var(--color-surface)] p-6 shadow-2xl ring-1 ring-inset ring-[var(--color-border-subtle)]"
      >
        <h2 className="text-[16px] font-semibold">登录微软账号</h2>

        {phase.kind === "starting" && (
          <p className="mt-3 flex items-center gap-2 text-[13px] text-[var(--color-text-secondary)]">
            <Spinner />
            正在向微软申请登录代码…
          </p>
        )}

        {phase.kind === "code" && (
          <>
            <p className="mt-3 text-[13px] leading-relaxed text-[var(--color-text-secondary)]">
              在浏览器中打开下面的网址，输入代码完成登录。启动器不需要你的密码。
            </p>

            <div className="mt-4 rounded-xl bg-[var(--color-surface-raised)] p-4 ring-1 ring-inset ring-[var(--color-border-subtle)]">
              <div className="text-[12px] text-[var(--color-text-muted)]">代码</div>
              <div className="mt-1 select-all font-mono text-[26px] tracking-[0.12em] text-[var(--color-text-primary)]">
                {phase.userCode}
              </div>
              <div className="mt-3 text-[12px] text-[var(--color-text-muted)]">网址</div>
              <ExternalLink href={phase.verificationUri} className="mt-0.5 block break-all text-[13px] text-[var(--color-accent-text)] hover:underline">
                {phase.verificationUri}
              </ExternalLink>
            </div>

            <p className="mt-3 flex items-center gap-2 text-[12px] text-[var(--color-text-secondary)]">
              <Spinner />
              等待你在浏览器里完成登录…
            </p>
          </>
        )}

        {phase.kind === "failed" && (
          <p className="mt-3 break-words text-[13px] text-[var(--color-danger)]">{phase.message}</p>
        )}

        <div className="mt-5 flex justify-end gap-2">
          <button
            type="button"
            onClick={onCancel}
            className="rounded-lg px-3.5 py-1.5 text-[13px] text-[var(--color-text-secondary)] ring-1 ring-inset ring-[var(--color-border-strong)] transition-colors hover:bg-[var(--color-surface-hover)]"
          >
            {phase.kind === "failed" ? "关闭" : "取消"}
          </button>
        </div>
      </div>
    </div>
  );
}

function Spinner() {
  return (
    <span
      aria-hidden="true"
      className="inline-block h-3.5 w-3.5 animate-spin rounded-full border-2 border-[var(--color-border-strong)] border-t-[var(--color-accent)]"
    />
  );
}
