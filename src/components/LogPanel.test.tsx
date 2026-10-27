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

describe("LogPanel 筛选与复制", () => {
  const MIXED: LogEntry[] = [
    { id: 1, stream: "stdout", text: "[1] [main/INFO]: Loading 63 mods" },
    { id: 2, stream: "stdout", text: "[2] [main/WARN]: mod 有点旧" },
    { id: 3, stream: "stdout", text: "[3] [main/ERROR]: 缺前置" },
  ];

  it("按级别筛选只留下对应行", () => {
    render(<LogPanel logs={MIXED} onClear={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "警告" }));

    expect(screen.getByText(/mod 有点旧/)).toBeInTheDocument();
    expect(screen.queryByText(/Loading 63 mods/)).toBeNull();
    expect(screen.queryByText(/缺前置/)).toBeNull();
  });

  it("筛选后显示「可见 / 总数」", () => {
    render(<LogPanel logs={MIXED} onClear={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "错误" }));

    expect(screen.getByText("1 / 3 行")).toBeInTheDocument();
  });

  it("关键词筛选忽略大小写", () => {
    render(<LogPanel logs={MIXED} onClear={vi.fn()} />);

    fireEvent.change(screen.getByLabelText("筛选日志"), { target: { value: "MODS" } });

    expect(screen.getByText(/Loading 63 mods/)).toBeInTheDocument();
    expect(screen.queryByText(/缺前置/)).toBeNull();
  });

  it("筛不出东西时给出提示而不是空白", () => {
    render(<LogPanel logs={MIXED} onClear={vi.fn()} />);

    fireEvent.change(screen.getByLabelText("筛选日志"), { target: { value: "不存在的词" } });

    expect(screen.getByText("没有符合条件的日志。")).toBeInTheDocument();
  });

  it("复制的是筛选后的内容，不是全部", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });

    render(<LogPanel logs={MIXED} onClear={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "错误" }));
    fireEvent.click(screen.getByRole("button", { name: "复制" }));

    await vi.waitFor(() => expect(writeText).toHaveBeenCalled());
    // 排查时最常见的用法：筛出错误再复制，所以只能复制看得见的行。
    expect(writeText).toHaveBeenCalledWith("[3] [main/ERROR]: 缺前置");
  });

  it("没有日志时不显示筛选条", () => {
    render(<LogPanel logs={[]} onClear={vi.fn()} />);

    expect(screen.queryByLabelText("筛选日志")).toBeNull();
    expect(screen.queryByRole("button", { name: "警告" })).toBeNull();
  });
});
