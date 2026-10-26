// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { LoaderOption } from "../api";
import { LoaderSelect } from "./LoaderSelect";

const LOADERS: LoaderOption[] = [
  { version: "0.20.1", stable: true },
  { version: "0.21.0-beta.1", stable: false },
];

describe("LoaderSelect", () => {
  it("始终提供「无（原版）」这一项", () => {
    render(
      <LoaderSelect loaders={[]} value="" busy={false} disabled={false} onChange={vi.fn()} />,
    );

    expect(screen.getByRole("option", { name: "无（原版）" })).toBeInTheDocument();
    expect(screen.getByRole("combobox")).toHaveValue("");
  });

  it("列出加载器版本并标注测试版", () => {
    render(
      <LoaderSelect
        loaders={LOADERS}
        value=""
        busy={false}
        disabled={false}
        onChange={vi.fn()}
      />,
    );

    expect(screen.getByRole("option", { name: "Fabric 0.20.1" })).toBeInTheDocument();
    // 测试版必须显眼：用户选了测试版出问题时要能一眼看出是自己选的
    expect(screen.getByRole("option", { name: "Fabric 0.21.0-beta.1（测试版）" })).toBeInTheDocument();
  });

  it("选择后回传版本号", async () => {
    const onChange = vi.fn();
    render(
      <LoaderSelect
        loaders={LOADERS}
        value=""
        busy={false}
        disabled={false}
        onChange={onChange}
      />,
    );

    await userEvent.selectOptions(screen.getByRole("combobox"), "0.20.1");

    expect(onChange).toHaveBeenCalledWith("0.20.1");
  });

  it("读取中时禁用并提示", () => {
    render(
      <LoaderSelect loaders={[]} value="" busy disabled={false} onChange={vi.fn()} />,
    );

    expect(screen.getByRole("combobox")).toBeDisabled();
    expect(screen.getByText("读取中…")).toBeInTheDocument();
  });

  it("安装或运行期间禁用", () => {
    render(
      <LoaderSelect loaders={LOADERS} value="" busy={false} disabled onChange={vi.fn()} />,
    );

    expect(screen.getByRole("combobox")).toBeDisabled();
  });
});
