// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import { applyInitialTheme } from "./lib/theme";
import "./styles.css";

// 在首帧之前定好主题：否则会先按默认深色渲染，再跳到用户上次选的浅色。
applyInitialTheme();

const container = document.getElementById("root");
if (!container) {
  throw new Error("找不到 #root 容器");
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

// 窗口在配置里是 `visible: false`——不这么做会先看到一片白底再变成界面。
// 等两帧：第一帧完成渲染，第二帧确保样式已生效，再显示窗口。
requestAnimationFrame(() => {
  requestAnimationFrame(() => {
    void showWindow();
  });
});

/** 显示主窗口。浏览器预览页里没有 Tauri，静默跳过。 */
async function showWindow(): Promise<void> {
  if (!("__TAURI_INTERNALS__" in window)) {
    return;
  }
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().show();
  } catch {
    console.error("显示窗口失败");
  }
}
