// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { VersionSummary } from "../api";
import { VersionList } from "./VersionList";

const VERSIONS: VersionSummary[] = [
  { id: "26.3", type: "release", releaseTime: "2026-09-15T11:23:02+00:00" },
  { id: "26.4-snapshot-1", type: "snapshot", releaseTime: "2026-09-22T00:00:00+00:00" },
];

describe("VersionList", () => {
  it("渲染版本号、中文类型与数量", () => {
    render(<VersionList versions={VERSIONS} selected={null} disabled={false} onSelect={vi.fn()} />);

    expect(screen.getByText("26.3")).toBeInTheDocument();
    expect(screen.getByText("正式版")).toBeInTheDocument();
    expect(screen.getByText("快照")).toBeInTheDocument();
    expect(screen.getByText("共 2 个")).toBeInTheDocument();
  });

  it("点击时回传版本号", async () => {
    const onSelect = vi.fn();
    render(<VersionList versions={VERSIONS} selected={null} disabled={false} onSelect={onSelect} />);

    await userEvent.click(screen.getByText("26.3"));

    expect(onSelect).toHaveBeenCalledWith("26.3");
  });

  it("禁用时不触发选择", async () => {
    // 安装或游戏运行期间必须锁住选择，否则进度与目标版本会对不上。
    const onSelect = vi.fn();
    render(<VersionList versions={VERSIONS} selected={null} disabled onSelect={onSelect} />);

    await userEvent.click(screen.getByText("26.3"));

    expect(onSelect).not.toHaveBeenCalled();
  });

  it("选中项标记为 aria-pressed", () => {
    render(<VersionList versions={VERSIONS} selected="26.3" disabled={false} onSelect={vi.fn()} />);

    expect(screen.getByRole("button", { name: /26\.3/ })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: /26\.4-snapshot-1/ })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });
});
