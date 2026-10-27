// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { StatusStack } from "./StatusStack";

function renderStack(overrides: Partial<Parameters<typeof StatusStack>[0]> = {}) {
  const props = {
    progress: null,
    message: null,
    error: null,
    mirrored: false,
    onDismissMessage: vi.fn(),
    onDismissError: vi.fn(),
    ...overrides,
  };
  render(<StatusStack {...props} />);
  return props;
}

describe("StatusStack", () => {
  it("什么都没有时不渲染任何东西", () => {
    // 浮层空着还占位的话，右下角的点击会被它挡住。
    const { container } = render(
      <StatusStack
        progress={null}
        message={null}
        error={null}
        mirrored={false}
        onDismissMessage={vi.fn()}
        onDismissError={vi.fn()}
      />,
    );

    expect(container).toBeEmptyDOMElement();
  });

  it("下载中显示阶段、进度与百分比", () => {
    renderStack({ progress: { phase: "assets", completed: 515, total: 5147, percent: 10 } });

    expect(screen.getByText("正在下载资产 515/5147")).toBeInTheDocument();
    expect(screen.getByText("10%")).toBeInTheDocument();
  });

  it("走镜像时标注 BMCLAPI 来源", () => {
    // BMCLAPI 使用协议第 3 条：使用它的程序必须在下载界面或其他可视部分标明来源。
    renderStack({
      progress: { phase: "versionFiles", completed: 3, total: 10, percent: 30 },
      mirrored: true,
    });

    expect(screen.getByRole("link", { name: "BMCLAPI" })).toBeInTheDocument();
  });

  it("官方源不标注", () => {
    renderStack({ progress: { phase: "assets", completed: 1, total: 2, percent: 50 } });

    expect(screen.queryByRole("link", { name: "BMCLAPI" })).toBeNull();
  });

  it("错误可以手动关掉", async () => {
    const props = renderStack({ error: "安装失败：磁盘已满" });

    expect(screen.getByRole("alert")).toHaveTextContent("安装失败：磁盘已满");
    await userEvent.click(screen.getByRole("button", { name: "关闭提示" }));

    expect(props.onDismissError).toHaveBeenCalled();
  });

  it("成功提示也能关掉", async () => {
    const props = renderStack({ message: "设置已保存" });

    await userEvent.click(screen.getByRole("button", { name: "关闭提示" }));

    expect(props.onDismissMessage).toHaveBeenCalled();
  });

  it("进度与错误可以同时存在", () => {
    // 下载失败时进度卡可能还没撤掉，两张卡要能叠着显示，不能互相顶掉。
    renderStack({
      progress: { phase: "assets", completed: 1, total: 10, percent: 10 },
      error: "网络错误",
    });

    expect(screen.getByText("正在下载资产 1/10")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("网络错误");
  });
});
