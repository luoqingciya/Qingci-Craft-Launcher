// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { useRef, useState } from "react";

import { formatSummary, type CrashSummary } from "../lib/crash";

/**
 * 崩溃摘要卡片。
 *
 * 放在日志上方：用户点「启动」后如果游戏崩了，第一眼应该看到「为什么」，
 * 而不是自己去几千行里翻。完整日志仍然在下面，想看细节随时能看。
 */
export function CrashCard({
  summary,
  onDismiss,
}: {
  summary: CrashSummary;
  onDismiss: () => void;
}) {
  const textRef = useRef<HTMLPreElement>(null);
  const [copied, setCopied] = useState<"idle" | "done" | "manual">("idle");

  async function copy() {
    const text = formatSummary(summary);
    try {
      await navigator.clipboard.writeText(text);
      setCopied("done");
    } catch {
      // 剪贴板 API 在非安全上下文里会被拒。退而求其次：把文本选中，让用户自己按 Ctrl+C。
      const element = textRef.current;
      if (element) {
        const range = document.createRange();
        range.selectNodeContents(element);
        const selection = window.getSelection();
        selection?.removeAllRanges();
        selection?.addRange(range);
      }
      setCopied("manual");
    }
  }

  return (
    <section className="mb-3 shrink-0 rounded-xl bg-[var(--color-danger)]/10 p-3.5 ring-1 ring-inset ring-[var(--color-danger)]/30">
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <h3 className="text-[13px] font-medium text-[var(--color-danger)]">
            {summary.headline}
          </h3>

          {summary.lines.length > 0 && (
            <pre
              ref={textRef}
              className="mt-2 max-h-40 select-text overflow-y-auto whitespace-pre-wrap break-all font-mono text-[12px] leading-relaxed text-[var(--color-text-secondary)]"
            >
              {summary.lines.join("\n")}
            </pre>
          )}
        </div>

        <div className="flex shrink-0 flex-col items-end gap-1">
          <button
            type="button"
            onClick={() => void copy()}
            className="rounded-md px-2 py-1 text-[11px] text-[var(--color-text-secondary)] ring-1 ring-inset ring-[var(--color-border-strong)] hover:bg-[var(--color-surface-hover)]"
          >
            {copied === "done" ? "已复制" : "复制摘要"}
          </button>
          <button
            type="button"
            onClick={onDismiss}
            className="rounded-md px-2 py-1 text-[11px] text-[var(--color-text-muted)] hover:bg-[var(--color-surface-hover)]"
          >
            收起
          </button>
        </div>
      </div>

      {copied === "manual" && (
        <p className="mt-2 text-[11px] text-[var(--color-text-muted)]">
          已选中文本，按 Ctrl+C 复制。
        </p>
      )}
    </section>
  );
}
