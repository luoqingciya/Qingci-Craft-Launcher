// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

/**
 * 主题：深色 / 浅色。
 *
 * 只做两态而不是「跟随系统 + 三态循环」：桌面应用里用户想切换时，
 * 要的是「现在变亮」，而不是在一个循环按钮上猜下一次会变成什么。
 * 首次启动跟随系统，一旦用户手动选过就记住他的选择。
 */

/** 用户可选的主题。 */
export type Theme = "light" | "dark";

const STORAGE_KEY = "qcl-theme";

/** 系统当前偏好。非浏览器环境（测试）默认深色。 */
export function systemTheme(): Theme {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return "dark";
  }
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

/** 读取用户上次的选择；没选过则跟随系统。 */
export function loadTheme(): Theme {
  try {
    const saved = window.localStorage.getItem(STORAGE_KEY);
    if (saved === "light" || saved === "dark") {
      return saved;
    }
  } catch {
    // 隐私模式等场景下 localStorage 会抛错，退回系统偏好即可。
  }
  return systemTheme();
}

/**
 * 应用主题。
 *
 * 三件事必须一起做，否则会出现「界面变了但滚动条/表单控件还是另一套」：
 * 1. 在 `<html>` 上打 `data-theme`，驱动 CSS 变量
 * 2. 同步窗口自身的主题（Windows 的标题栏会跟着变）
 * 3. 记住选择
 */
export function applyTheme(theme: Theme): void {
  const root = document.documentElement;
  root.dataset.theme = theme;

  try {
    window.localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // 存不下就只在本次会话生效，不影响功能。
  }

  // 浏览器预览页里没有 Tauri，动态导入失败是正常的。
  void syncWindowTheme(theme);
}

/** 让窗口标题栏跟随应用主题。非 Tauri 环境静默跳过。 */
async function syncWindowTheme(theme: Theme): Promise<void> {
  if (!("__TAURI_INTERNALS__" in window)) {
    return;
  }
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().setTheme(theme);
  } catch {
    // 权限缺失或平台不支持时忽略：界面主题已经生效，标题栏跟随只是加分项。
  }
}

/** 在首帧之前应用主题，避免先渲染深色再跳到浅色。 */
export function applyInitialTheme(): Theme {
  const theme = loadTheme();
  document.documentElement.dataset.theme = theme;
  void syncWindowTheme(theme);
  return theme;
}
