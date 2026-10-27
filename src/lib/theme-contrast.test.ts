// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 主题配色的对比度护栏。
//!
//! 为什么要写成测试：深浅两套主题各有一份变量，改一版配色很容易只顾着「好看」，
//! 把 12px 的提示文字调到 3:1 以下——那种问题在开发者的高分屏上不明显，
//! 在别人的笔记本上就是「看不清」。这条测试把 WCAG AA 的 4.5:1 变成硬约束。
//!
//! 只测真正用来读文字的几个变量，装饰性的边框、悬浮态不在此列。

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

/** 从 CSS 里取某个主题块中某个变量的十六进制值。 */
function pick(block: string, name: string): string {
  const match = block.match(new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{6})`));
  if (!match) {
    throw new Error(`样式里找不到 --${name}`);
  }
  return match[1];
}

/** 相对亮度（WCAG 2.1 定义）。 */
function luminance(hex: string): number {
  const channels = hex
    .slice(1)
    .match(/../g)!
    .map((pair) => Number.parseInt(pair, 16) / 255)
    .map((value) => (value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4));
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

/** 两色的对比度。 */
function contrast(foreground: string, background: string): number {
  const a = luminance(foreground);
  const b = luminance(background);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

const css = readFileSync(join(process.cwd(), "src/styles.css"), "utf8");

// 从行首的 `[data-theme="light"] {` 开始切——直接 indexOf 选择器会命中文件顶部
// 注释里提到它的那一处，切出来是空块。
const lightStart = css.search(/^\[data-theme="light"\]\s*\{/m);
if (lightStart < 0) {
  throw new Error("样式里找不到浅色主题块");
}
const darkBlock = css.slice(css.indexOf("@theme"), lightStart);
const lightBlock = css.slice(lightStart);

describe("主题对比度", () => {
  // AA 对正文的门槛是 4.5:1。界面里最小的是 11–12px 的提示文字，属于「正文」。
  const AA = 4.5;

  it.each([
    ["深色", darkBlock],
    ["浅色", lightBlock],
  ])("%s：正文与次级文字在两种底色上都达标", (_label, block) => {
    const backgrounds = ["color-surface", "color-surface-raised", "color-canvas-from"];
    for (const fg of ["color-text-primary", "color-text-secondary", "color-text-muted"]) {
      for (const bg of backgrounds) {
        const ratio = contrast(pick(block, fg), pick(block, bg));
        expect(
          ratio,
          `--${fg} 在 --${bg} 上只有 ${ratio.toFixed(2)}:1`,
        ).toBeGreaterThanOrEqual(AA);
      }
    }
  });

  it.each([
    ["深色", darkBlock],
    ["浅色", lightBlock],
  ])("%s：强调色文字与语义色在卡片上可读", (_label, block) => {
    const raised = pick(block, "color-surface-raised");
    for (const name of ["color-accent-text", "color-success", "color-danger", "color-warning"]) {
      const ratio = contrast(pick(block, name), raised);
      expect(ratio, `--${name} 在卡片上只有 ${ratio.toFixed(2)}:1`).toBeGreaterThanOrEqual(AA);
    }
  });
});
