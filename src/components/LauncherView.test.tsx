// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { render, screen, within } from "@testing-library/react";
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
  it("显示版本数量与未运行状态", () => {
    renderView();

    expect(screen.getByText("2 个")).toBeInTheDocument();
    expect(screen.getByText("未运行")).toBeInTheDocument();
  });

  it("未选版本时启动按钮禁用", () => {
    renderView();

    expect(screen.getByRole("button", { name: "启动游戏" })).toBeDisabled();
    expect(screen.getByText("未选择")).toBeInTheDocument();
  });

  it("选中版本后启动可用，并显示版本号", () => {
    renderView({ selected: "26.3" });

    expect(screen.getByRole("button", { name: "启动游戏" })).toBeEnabled();

    // 「当前版本」与左侧列表都会出现 26.3，按容器定位避免撞车。
    const current = screen.getByText("当前版本").parentElement;
    expect(current).not.toBeNull();
    expect(within(current as HTMLElement).getByText("26.3")).toBeInTheDocument();
  });

  it("运行中时启动禁用、停止可用，状态显示版本号", () => {
    renderView({ selected: "26.3", status: { running: true, version: "26.3" } });

    expect(screen.getByRole("button", { name: "游戏运行中" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "停止" })).toBeEnabled();
    expect(screen.getByRole("status")).toHaveTextContent("26.3");
  });

  it("选了加载器时显示 Fabric 标签", () => {
    renderView({ selected: "26.3", loader: "0.20.1" });

    expect(screen.getByText("Fabric 0.20.1")).toBeInTheDocument();
  });

  it("默认停在「启动」页，切换标签页才显示设置与关于", async () => {
    renderView({ selected: "26.3" });

    expect(screen.getByRole("tab", { name: "启动" })).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByText("游戏目录")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: "设置" }));
    expect(screen.getByText("游戏目录")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: "关于" }));
    expect(screen.getByText(/GPL-3\.0/)).toBeInTheDocument();
    expect(screen.queryByText("游戏目录")).not.toBeInTheDocument();
  });

  it("主题按钮在深色/浅色之间切换", async () => {
    renderView();

    const toggle = screen.getByRole("button", { name: "切换到浅色模式" });
    await userEvent.click(toggle);

    // 切换后按钮的语义与 <html data-theme> 都要跟着变，否则界面不会真的变。
    expect(screen.getByRole("button", { name: "切换到深色模式" })).toBeInTheDocument();
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("版本清单加载失败时给出错误，而不是空列表", () => {
    renderView({ versions: { status: "failed", message: "网络错误" } });

    expect(screen.getByText(/获取版本清单失败/)).toBeInTheDocument();
  });
});
