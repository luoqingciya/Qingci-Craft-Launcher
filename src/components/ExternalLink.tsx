// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import type { ReactNode } from "react";

/**
 * 在系统浏览器里打开外部链接。
 *
 * Tauri 默认拦截外部导航，必须走 opener 插件；浏览器预览页里没有这个插件，
 * 退回 `window.open`。
 */
async function openExternal(href: string) {
  try {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(href);
  } catch {
    window.open(href, "_blank", "noreferrer");
  }
}

/** 外部链接。 */
export function ExternalLink({
  href,
  children,
  className = "",
}: {
  href: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <a
      href={href}
      onClick={(event) => {
        event.preventDefault();
        void openExternal(href);
      }}
      className={
        className || "w-fit text-[13px] text-[var(--color-accent-text)] hover:underline"
      }
    >
      {children}
    </a>
  );
}
