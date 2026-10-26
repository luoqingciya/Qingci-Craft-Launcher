// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ModsTab, formatDownloads } from "./ModsTab";

const HITS = [
  {
    projectId: "AANobbMI",
    title: "Sodium",
    description: "高性能渲染引擎",
    downloads: 123_456_789,
    author: "jellysquid3",
  },
  {
    projectId: "gvQqBUqZ",
    title: "Lithium",
    description: "通用性能优化",
    downloads: 45_600,
    author: "jellysquid3",
  },
];

const searchMods = vi.fn();
const installMod = vi.fn();
const listMods = vi.fn();

vi.mock("../api", () => ({
  searchMods: (...args: unknown[]) => searchMods(...args),
  installMod: (...args: unknown[]) => installMod(...args),
  listMods: () => listMods(),
}));

beforeEach(() => {
  searchMods.mockReset();
  installMod.mockReset();
  listMods.mockReset();
  listMods.mockResolvedValue([]);
});

describe("ModsTab", () => {
  it("原版（没有加载器）时提示先换加载器，而不是给一个装了也没用的入口", () => {
    render(<ModsTab version="26.3" loader="" />);

    expect(screen.getByText(/原版，模组不会被加载/)).toBeInTheDocument();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  });

  it("没选版本时提示先选版本", () => {
    render(<ModsTab version={null} loader="0.20.1" />);

    expect(screen.getByText(/先选择一个版本/)).toBeInTheDocument();
  });

  it("搜索时带上游戏版本与 fabric 分类", async () => {
    searchMods.mockResolvedValue(HITS);
    render(<ModsTab version="26.3" loader="0.20.1" />);

    await userEvent.type(screen.getByRole("textbox"), "sodium{Enter}");

    expect(searchMods).toHaveBeenCalledWith("sodium", "26.3", "fabric");
    expect(await screen.findByText("Sodium")).toBeInTheDocument();
    expect(screen.getByText("高性能渲染引擎")).toBeInTheDocument();
  });

  it("安装后提示落地路径并刷新已装列表", async () => {
    searchMods.mockResolvedValue(HITS);
    installMod.mockResolvedValue("mods/sodium.jar");
    listMods.mockResolvedValueOnce([]).mockResolvedValueOnce(["sodium.jar"]);

    render(<ModsTab version="26.3" loader="0.20.1" />);
    await userEvent.type(screen.getByRole("textbox"), "sodium{Enter}");
    const buttons = await screen.findAllByRole("button", { name: "安装" });
    await userEvent.click(buttons[0]);

    expect(installMod).toHaveBeenCalledWith("AANobbMI", "26.3", "fabric");
    expect(await screen.findByText("已安装到 mods/sodium.jar")).toBeInTheDocument();
    expect(await screen.findByText("sodium.jar")).toBeInTheDocument();
  });

  it("搜索失败时显示错误而不是空白", async () => {
    searchMods.mockRejectedValue("网络错误");
    render(<ModsTab version="26.3" loader="0.20.1" />);

    await userEvent.type(screen.getByRole("textbox"), "x{Enter}");

    expect(await screen.findByText("网络错误")).toBeInTheDocument();
  });

  it("没有结果时给出提示", async () => {
    searchMods.mockResolvedValue([]);
    render(<ModsTab version="26.3" loader="0.20.1" />);

    await userEvent.type(screen.getByRole("textbox"), "zzz{Enter}");

    expect(await screen.findByText(/没有找到适配该版本的模组/)).toBeInTheDocument();
  });

  it("空查询不发请求", async () => {
    render(<ModsTab version="26.3" loader="0.20.1" />);

    await userEvent.type(screen.getByRole("textbox"), "{Enter}");

    expect(searchMods).not.toHaveBeenCalled();
  });
});

describe("formatDownloads", () => {
  it("按量级换单位", () => {
    expect(formatDownloads(999)).toBe("999");
    expect(formatDownloads(45_600)).toBe("4.6 万");
    expect(formatDownloads(123_456_789)).toBe("1.2 亿");
  });
});
