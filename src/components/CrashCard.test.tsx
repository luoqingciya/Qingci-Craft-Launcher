// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { CrashSummary } from "../lib/crash";
import { CrashCard } from "./CrashCard";

const SUMMARY: CrashSummary = {
  exitCode: 1,
  headline: "模组依赖或兼容性有问题，游戏在加载阶段退出",
  lines: [
    "[main/ERROR]: Incompatible mod set!",
    "\t- Mod 'Sodium' requires version 1.0.0 of fabric-api, which is missing!",
  ],
};

describe("CrashCard", () => {
  it("显示结论与关键行", () => {
    render(<CrashCard summary={SUMMARY} onDismiss={vi.fn()} />);

    expect(screen.getByText(/模组依赖或兼容性有问题/)).toBeInTheDocument();
    expect(screen.getByText(/Incompatible mod set/)).toBeInTheDocument();
    expect(screen.getByText(/fabric-api, which is missing/)).toBeInTheDocument();
  });

  it("复制按钮把摘要写进剪贴板", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    render(<CrashCard summary={SUMMARY} onDismiss={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "复制摘要" }));

    expect(writeText).toHaveBeenCalledTimes(1);
    const copied = writeText.mock.calls[0][0] as string;
    expect(copied).toContain("退出码 1");
    expect(copied).toContain("Incompatible mod set");
    expect(await screen.findByRole("button", { name: "已复制" })).toBeInTheDocument();
  });

  it("剪贴板不可用时退化为选中文本", async () => {
    // 非安全上下文里 clipboard 会被拒，不能静默失败——要告诉用户手动复制。
    Object.assign(navigator, {
      clipboard: { writeText: vi.fn().mockRejectedValue(new Error("denied")) },
    });

    render(<CrashCard summary={SUMMARY} onDismiss={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "复制摘要" }));

    expect(await screen.findByText(/按 Ctrl\+C 复制/)).toBeInTheDocument();
  });

  it("可以收起", async () => {
    const onDismiss = vi.fn();
    render(<CrashCard summary={SUMMARY} onDismiss={onDismiss} />);

    await userEvent.click(screen.getByRole("button", { name: "收起" }));

    expect(onDismiss).toHaveBeenCalled();
  });

  it("没有关键行时只显示结论", () => {
    render(
      <CrashCard
        summary={{ exitCode: 1, headline: "游戏异常退出", lines: [] }}
        onDismiss={vi.fn()}
      />,
    );

    expect(screen.getByText("游戏异常退出")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "复制摘要" })).toBeInTheDocument();
  });
});
