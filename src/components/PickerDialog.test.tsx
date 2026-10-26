// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { PickerDialog, type PickerItem } from "./PickerDialog";

function items(count: number): PickerItem[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `26.${count - index}`,
    label: `26.${count - index}`,
    meta: "2026-09-15",
  }));
}

const FEW: PickerItem[] = [
  { id: "26.3", label: "26.3", meta: "2026-09-15" },
  { id: "26.4-snapshot-1", label: "26.4-snapshot-1", meta: "2026-10-22", badge: "快照" },
];

function renderPicker(overrides: Partial<Parameters<typeof PickerDialog>[0]> = {}) {
  const props = {
    title: "选择版本",
    placeholder: "搜索",
    items: FEW,
    selectedId: null as string | null,
    onPick: vi.fn(),
    onClose: vi.fn(),
    ...overrides,
  };
  render(<PickerDialog {...props} />);
  return props;
}

describe("PickerDialog", () => {
  it("输入即过滤，且不区分大小写", async () => {
    renderPicker();

    await userEvent.type(screen.getByRole("textbox"), "SNAPSHOT");

    expect(screen.getByText("26.4-snapshot-1")).toBeInTheDocument();
    expect(screen.queryByText("26.3")).not.toBeInTheDocument();
  });

  it("点击选项即回传 id", async () => {
    const props = renderPicker();

    await userEvent.click(screen.getByText("26.3"));

    expect(props.onPick).toHaveBeenCalledWith("26.3");
  });

  it("↑↓ 移动高亮，Enter 选中当前项", async () => {
    const props = renderPicker();

    const input = screen.getByRole("textbox");
    await userEvent.type(input, "{ArrowDown}{Enter}");

    // 第一项是 26.3，按下 ↓ 后应选中第二项。
    expect(props.onPick).toHaveBeenCalledWith("26.4-snapshot-1");
  });

  it("Esc 关闭", async () => {
    const props = renderPicker();

    await userEvent.type(screen.getByRole("textbox"), "{Escape}");

    expect(props.onClose).toHaveBeenCalled();
  });

  it("无匹配时给出提示，而不是空白", async () => {
    renderPicker();

    await userEvent.type(screen.getByRole("textbox"), "zzz");

    expect(screen.getByText("没有匹配项")).toBeInTheDocument();
  });

  it("项数过多时只渲染前 60 项并提示继续输入", () => {
    // 九百多个版本全渲染会让打开选择器明显卡顿；这里验证截断确实生效。
    renderPicker({ items: items(120) });

    expect(screen.getByText("120 项")).toBeInTheDocument();
    expect(screen.getByText(/还有 60 项未显示/)).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /26\./ })).toHaveLength(60);
  });

  it("标出当前已选项", () => {
    renderPicker({ selectedId: "26.3" });

    expect(screen.getByText("已选")).toBeInTheDocument();
  });

  it("显示类型标签", () => {
    renderPicker();

    expect(screen.getByText("快照")).toBeInTheDocument();
  });
});
