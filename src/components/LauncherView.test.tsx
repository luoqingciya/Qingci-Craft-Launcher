// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { SettingsView, VersionSummary } from "../api";
import { LauncherView } from "./LauncherView";

const VERSIONS: VersionSummary[] = [
  { id: "26.3", type: "release", releaseTime: "2026-09-15T11:23:02+00:00" },
  { id: "26.4-snapshot-1", type: "snapshot", releaseTime: "2026-10-22T10:00:00+00:00" },
];

const SETTINGS: SettingsView = {
  gameDir: "C:\\game",
  defaultGameDir: "C:\\game",
  isCustom: false,
  fallbackReason: null,
  playerName: "清辞",
  memoryMb: null,
  javaPath: null,
  memoryRange: [512, 65536],
};

function renderView(overrides: Partial<Parameters<typeof LauncherView>[0]> = {}) {
  const props = {
    versions: { status: "ready" as const, versions: VERSIONS },
    selected: null as string | null,
    loaders: [],
    loader: "",
    loaderBusy: false,
    busy: false,
    progress: null,
    status: { running: false, version: null },
    logs: [],
    settings: SETTINGS,
    javaOptions: [],
    savingSettings: false,
    settingsError: null,
    message: null,
    error: null,
    onSelectVersion: vi.fn(),
    onLoaderChange: vi.fn(),
    onInstall: vi.fn(),
    onLaunch: vi.fn(),
    onStop: vi.fn(),
    onClearLogs: vi.fn(),
    onSaveSettings: vi.fn(),
    onRefreshJava: vi.fn(),
    ...overrides,
  };
  render(<LauncherView {...props} />);
  return props;
}

describe("LauncherView", () => {
  it("左栏显示账户名与离线标记，顶栏显示未运行", () => {
    renderView();

    expect(screen.getByText("清辞")).toBeInTheDocument();
    expect(screen.getByText("离线账户")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("未运行");
  });

  it("未选版本时启动按钮禁用，并提示先选版本", () => {
    renderView();

    expect(screen.getByRole("button", { name: /启动游戏/ })).toBeDisabled();
    expect(screen.getByText("先选择一个版本")).toBeInTheDocument();
  });

  it("选中版本后启动可用，按钮副标题显示版本", () => {
    renderView({ selected: "26.3" });

    expect(screen.getByRole("button", { name: /启动游戏/ })).toBeEnabled();
    // 「当前版本」区块与按钮副标题都会出现 26.3。
    expect(screen.getAllByText("26.3").length).toBeGreaterThanOrEqual(2);
  });

  it("运行中时启动禁用、停止可用，状态显示版本号", () => {
    renderView({ selected: "26.3", status: { running: true, version: "26.3" } });

    expect(screen.getByRole("button", { name: /游戏运行中/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "停止" })).toBeEnabled();
    expect(screen.getByRole("status")).toHaveTextContent("26.3");
  });

  it("选了加载器时按钮副标题带上 Fabric", () => {
    renderView({ selected: "26.3", loader: "0.20.1" });

    expect(screen.getByText("26.3 · Fabric 0.20.1")).toBeInTheDocument();
  });

  it("版本列表只在「版本」页出现", async () => {
    renderView({ selected: "26.3" });

    expect(screen.queryByText("2 个")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: "版本" }));
    expect(screen.getByText("2 个")).toBeInTheDocument();
    expect(screen.getByText("26.4-snapshot-1")).toBeInTheDocument();
  });

  it("「选择…」按钮切到版本页", async () => {
    renderView({ selected: "26.3" });

    await userEvent.click(screen.getByRole("button", { name: "选择…" }));

    expect(screen.getByRole("tab", { name: "版本" })).toHaveAttribute("aria-selected", "true");
  });

  it("设置页与关于页各自渲染对应内容", async () => {
    renderView({ selected: "26.3" });

    await userEvent.click(screen.getByRole("tab", { name: "设置" }));
    expect(screen.getByText("游戏目录")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: "关于" }));
    expect(screen.getByText(/GPL-3\.0/)).toBeInTheDocument();
    expect(screen.queryByText("游戏目录")).not.toBeInTheDocument();
  });

  it("主题按钮在深色/浅色之间切换", async () => {
    renderView();

    await userEvent.click(screen.getByRole("button", { name: "切换到浅色模式" }));

    // 切换后按钮语义与 <html data-theme> 都要跟着变，否则界面不会真的变。
    expect(screen.getByRole("button", { name: "切换到深色模式" })).toBeInTheDocument();
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("版本清单加载失败时给出错误，而不是空列表", async () => {
    renderView({ versions: { status: "failed", message: "网络错误" } });

    await userEvent.click(screen.getByRole("tab", { name: "版本" }));

    expect(screen.getByText(/获取版本清单失败/)).toBeInTheDocument();
  });
});
