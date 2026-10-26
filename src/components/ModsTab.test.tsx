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
    iconUrl: "https://cdn.modrinth.com/data/AANobbMI/icon.png",
  },
  {
    projectId: "gvQqBUqZ",
    title: "Lithium",
    description: "通用性能优化",
    downloads: 45_600,
    author: "jellysquid3",
    iconUrl: null,
  },
];

const searchMods = vi.fn();
const installMod = vi.fn();
const listMods = vi.fn();
const removeMod = vi.fn();

vi.mock("../api", () => ({
  searchMods: (...args: unknown[]) => searchMods(...args),
  installMod: (...args: unknown[]) => installMod(...args),
  listMods: () => listMods(),
  removeMod: (...args: unknown[]) => removeMod(...args),
}));

beforeEach(() => {
  searchMods.mockReset();
  installMod.mockReset();
  listMods.mockReset();
  removeMod.mockReset();
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

describe("卸载", () => {
  it("删除是两步的：先确认，再真的删", async () => {
    listMods.mockResolvedValue(["sodium.jar"]);
    removeMod.mockResolvedValue(undefined);

    render(<ModsTab version="26.3" loader="0.20.1" />);
    const deleteButton = await screen.findByRole("button", { name: "删除" });

    await userEvent.click(deleteButton);
    // 只点一次不该动文件——误点一次就删掉是灾难。
    expect(removeMod).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "确认删除" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "确认删除" }));
    expect(removeMod).toHaveBeenCalledWith("sodium.jar");
  });

  it("可以取消删除", async () => {
    listMods.mockResolvedValue(["sodium.jar"]);
    render(<ModsTab version="26.3" loader="0.20.1" />);

    await userEvent.click(await screen.findByRole("button", { name: "删除" }));
    await userEvent.click(screen.getByRole("button", { name: "取消" }));

    expect(screen.queryByRole("button", { name: "确认删除" })).not.toBeInTheDocument();
    expect(removeMod).not.toHaveBeenCalled();
  });

  it("删除失败时显示错误", async () => {
    listMods.mockResolvedValue(["sodium.jar"]);
    removeMod.mockRejectedValue("文件被占用");

    render(<ModsTab version="26.3" loader="0.20.1" />);
    await userEvent.click(await screen.findByRole("button", { name: "删除" }));
    await userEvent.click(screen.getByRole("button", { name: "确认删除" }));

    expect(await screen.findByText("文件被占用")).toBeInTheDocument();
  });
});
