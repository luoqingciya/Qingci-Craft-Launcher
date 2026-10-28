// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

/**
 * 复制文本到剪贴板。
 *
 * 先试 `navigator.clipboard`，失败再退回 `execCommand`：Tauri 的 WebView 在部分
 * 平台上不把 `tauri://` 视作安全上下文，clipboard API 会直接抛错。
 *
 * 返回是否成功——调用方据此决定要不要显示「已复制」。
 */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // 某些 WebView 里 clipboard API 不可用，退回旧办法。
    const area = document.createElement("textarea");
    area.value = text;
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    let ok = false;
    try {
      ok = document.execCommand("copy");
    } catch {
      ok = false;
    }
    document.body.removeChild(area);
    return ok;
  }
}
