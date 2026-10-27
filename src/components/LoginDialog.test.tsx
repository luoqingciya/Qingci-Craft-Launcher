// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { LoginDialog } from "./LoginDialog";

describe("LoginDialog", () => {
  it("空闲时不渲染任何东西", () => {
    // 对话框由阶段驱动，界面上不该残留一个空壳。
    render(<LoginDialog phase={{ kind: "idle" }} onCancel={vi.fn()} />);

    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("申请代码时说明正在做什么", () => {
    render(<LoginDialog phase={{ kind: "starting" }} onCancel={vi.fn()} />);

    expect(screen.getByText("正在向微软申请登录代码…")).toBeInTheDocument();
  });

  it("显示短码与验证地址，地址是可点的链接", () => {
    render(
      <LoginDialog
        phase={{
          kind: "code",
          userCode: "ABCD-1234",
          verificationUri: "https://microsoft.com/link",
          expiresIn: 900,
        }}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText("ABCD-1234")).toBeInTheDocument();
    const link = screen.getByRole("link", { name: "https://microsoft.com/link" });
    expect(link).toHaveAttribute("href", "https://microsoft.com/link");
  });

  it("等待阶段可以取消", async () => {
    const onCancel = vi.fn();
    render(
      <LoginDialog
        phase={{
          kind: "code",
          userCode: "ABCD-1234",
          verificationUri: "https://microsoft.com/link",
          expiresIn: 900,
        }}
        onCancel={onCancel}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(onCancel).toHaveBeenCalled();
  });

  it("失败时显示原因，按钮变成关闭", async () => {
    const onCancel = vi.fn();
    render(<LoginDialog phase={{ kind: "failed", message: "登录超时，请重新开始" }} onCancel={onCancel} />);

    expect(screen.getByText("登录超时，请重新开始")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "关闭" }));
    expect(onCancel).toHaveBeenCalled();
  });
});
