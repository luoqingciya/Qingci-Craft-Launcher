// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri 会以固定端口加载开发服务器，因此 strictPort 必须为 true：
// 端口被占用时宁可失败，也不要静默换端口导致窗口白屏。
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    // Tauri 的 WebView 版本可控（Windows 是 WebView2、macOS 是 WKWebView），
    // 不需要为老浏览器降级。
    target: "es2022",
    sourcemap: false,
  },
});
