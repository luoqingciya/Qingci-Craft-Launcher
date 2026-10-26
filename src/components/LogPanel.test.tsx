// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { LogEntry } from "../lib/logs";
import { LogPanel } from "./LogPanel";

function lines(count: number, offset = 0): LogEntry[] {
  return Array.from({ length: count }, (_, index) => ({
    id: offset + index + 1,
    stream: "stdout" as const,
    text: `第 ${offset + index + 1} 行`,
  }));
}

/**
 * jsdom 没有布局引擎，`scrollHeight` / `clientHeight` 恒为 0。
 * 这里手动给出「内容 1000px、可视 300px」的度量，才能验证跟随逻辑。
 */
function mockMetrics(element: HTMLElement, scrollHeight = 1000, clientHeight = 300) {
  Object.defineProperty(element, "scrollHeight", { value: scrollHeight, configurable: true });
  Object.defineProperty(element, "clientHeight", { value: clientHeight, configurable: true });
}

function renderPanel(initial: LogEntry[] = []) {
  const view = render(<LogPanel logs={initial} onClear={vi.fn()} />);
  const element = screen.getByRole("log");
  mockMetrics(element);
  return { ...view, element };
}

describe("LogPanel", () => {
  it("第一批日志就超过一屏时，仍然滚到底部", () => {
    // 这是原来的 bug：按「离底部多远」判断，首批日志到达时 scrollTop 还是 0，
    // 一算就是「不在底部」，于是永远不跟随。
    const { element, rerender } = renderPanel();

    rerender(<LogPanel logs={lines(60)} onClear={vi.fn()} />);

    expect(element.scrollTop).toBe(1000);
  });

  it("用户向上滚开后不再自动跟随", () => {
    const { element, rerender } = renderPanel(lines(60));

    // 用户往回翻。
    element.scrollTop = 100;
    fireEvent.scroll(element);

    rerender(<LogPanel logs={lines(61)} onClear={vi.fn()} />);

    expect(element.scrollTop).toBe(100);
  });

  it("用户滚回底部后恢复跟随", () => {
    const { element, rerender } = renderPanel(lines(60));

    element.scrollTop = 100;
    fireEvent.scroll(element);

    // 滚回底部（1000 - 700 - 300 = 0，在容差内）。
    element.scrollTop = 700;
    fireEvent.scroll(element);

    rerender(<LogPanel logs={lines(61)} onClear={vi.fn()} />);

    expect(element.scrollTop).toBe(1000);
  });

  it("清空后重新开始跟随", () => {
    const { element, rerender } = renderPanel(lines(60));

    element.scrollTop = 100;
    fireEvent.scroll(element);

    rerender(<LogPanel logs={[]} onClear={vi.fn()} />);
    rerender(<LogPanel logs={lines(10)} onClear={vi.fn()} />);

    expect(element.scrollTop).toBe(1000);
  });

  it("显示行数，达到上限时说明只保留最近若干行", () => {
    const { rerender } = renderPanel(lines(3));
    expect(screen.getByText("3 行")).toBeInTheDocument();

    rerender(<LogPanel logs={lines(2500)} onClear={vi.fn()} />);
    expect(screen.getByText("最近 2000 行")).toBeInTheDocument();
  });

  it("空日志给出提示而不是空白", () => {
    renderPanel();

    expect(screen.getByText(/启动游戏后/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "清空" })).toBeDisabled();
  });
});
