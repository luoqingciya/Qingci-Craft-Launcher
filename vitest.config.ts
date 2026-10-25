// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

/**
 * 测试专用配置。
 *
 * 不复用 `vite.config.ts`：那里挂了 Tailwind 插件，测试不需要它，
 * 还会拖慢启动。两处共享的只有 React 插件（JSX 转换）。
 */
export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    // 测试里不需要 CSS 处理，直接忽略样式导入的副作用。
    css: false,
  },
});
