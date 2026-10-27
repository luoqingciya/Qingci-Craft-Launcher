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
  selectedVersion: null,
  loaderVersion: null,
  downloadSource: "official",
};

function renderView(overrides: Partial<Parameters<typeof LauncherView>[0]> = {}) {
  const props = {
    versions: { status: "ready" as const, versions: VERSIONS },
    selected: null as string | null,
    loaders: [
      { version: "0.20.1", stable: true },
      { version: "0.21.0-beta.1", stable: false },
    ],
    loader: "",
    loaderBusy: false,
    busy: false,
    progress: null,
    status: { running: false, version: null },
    logs: [],
    crash: null,
    account: null,
    msaLogin: false,
    loginPhase: { kind: "idle" as const },
    update: { kind: "idle" as const },
    portable: false,
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
    onDismissCrash: vi.fn(),
    onLogin: vi.fn(),
    onCancelLogin: vi.fn(),
    onSignOut: vi.fn(),
    onCheckUpdate: vi.fn(),
    onInstallUpdate: vi.fn(),
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

  it("构建支持登录时，离线状态下给「登录」入口", () => {
    renderView({ msaLogin: true });

    expect(screen.getByRole("button", { name: "登录" })).toBeInTheDocument();
  });

  it("构建没内置客户端 ID 时不给登录入口", () => {
    // 给一个点了必然失败的按钮，比不给更糟。
    renderView({ msaLogin: false });

    expect(screen.queryByRole("button", { name: "登录" })).toBeNull();
  });

  it("已登录时显示账号名与正版标记，并能退出", async () => {
    const { onSignOut } = renderView({
      account: { name: "Notch", uuid: "069a79f444e94726a5befca90e38aaf5" },
      msaLogin: true,
    });

    expect(screen.getByText("Notch")).toBeInTheDocument();
    expect(screen.getByText("正版账户")).toBeInTheDocument();
    // 已登录时不该再出现登录入口
    expect(screen.queryByRole("button", { name: "登录" })).toBeNull();

    await userEvent.click(screen.getByRole("button", { name: "退出" }));
    expect(onSignOut).toHaveBeenCalled();
  });

  it("未选版本时主按钮是「选择版本」入口，点击打开选择器", async () => {
    renderView();

    // 点不动的按钮会让人以为程序坏了，所以未选版本时它是入口而不是死按钮。
    const button = screen.getByRole("button", { name: /^选择版本/ });
    expect(button).toBeEnabled();

    await userEvent.click(button);
    expect(screen.getByPlaceholderText("搜索版本号，如 1.20 或 26.3")).toBeInTheDocument();
  });

  it("在选择器里搜到并选中版本后关闭选择器", async () => {
    const props = renderView();

    await userEvent.click(screen.getByRole("button", { name: "更换版本" }));
    await userEvent.type(screen.getByRole("textbox"), "26.4");
    await userEvent.click(screen.getByText("26.4-snapshot-1"));

    expect(props.onSelectVersion).toHaveBeenCalledWith("26.4-snapshot-1");
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  });

  it("选中版本后主按钮显示启动，并显示版本号", () => {
    renderView({ selected: "26.3" });

    expect(screen.getByRole("button", { name: /^启动游戏/ })).toBeEnabled();
    // 版本行与按钮副标题都会显示 26.3。
    expect(screen.getAllByText("26.3").length).toBeGreaterThanOrEqual(2);
  });

  it("运行中时启动禁用、停止可用，状态显示版本号", () => {
    renderView({ selected: "26.3", status: { running: true, version: "26.3" } });

    expect(screen.getByRole("button", { name: /游戏运行中/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "停止" })).toBeEnabled();
    expect(screen.getByRole("status")).toHaveTextContent("26.3");
  });

  it("加载器选择器默认含「无（原版）」并列出 Fabric 版本", async () => {
    const props = renderView({ selected: "26.3" });

    await userEvent.click(screen.getByRole("button", { name: "更换加载器" }));

    // 左栏的加载器行与选择器里的「无（原版）」都会出现。
    expect(screen.getAllByText("无（原版）").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText("Fabric 0.20.1")).toBeInTheDocument();
    expect(screen.getByText("测试版")).toBeInTheDocument();

    await userEvent.click(screen.getByText("Fabric 0.20.1"));
    expect(props.onLoaderChange).toHaveBeenCalledWith("0.20.1");
  });

  it("选中加载器后主按钮副标题带上 Fabric", () => {
    renderView({ selected: "26.3", loader: "0.20.1" });

    expect(screen.getByText("26.3 · Fabric 0.20.1")).toBeInTheDocument();
  });

  it("未选版本时加载器入口禁用", () => {
    renderView();

    expect(screen.getByRole("button", { name: "更换加载器" })).toBeDisabled();
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

    expect(screen.getByRole("button", { name: "切换到深色模式" })).toBeInTheDocument();
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("版本清单加载失败时，选择器里给出提示而不是空白", async () => {
    renderView({ versions: { status: "failed", message: "网络错误" } });

    await userEvent.click(screen.getByRole("button", { name: "更换版本" }));

    expect(screen.getByText("版本清单还没加载完")).toBeInTheDocument();
  });
});

describe("崩溃摘要", () => {
  it("有摘要时在日志上方显示卡片", () => {
    renderView({
      selected: "26.3",
      crash: {
        exitCode: 1,
        headline: "模组依赖或兼容性有问题，游戏在加载阶段退出",
        lines: ["Incompatible mod set!"],
        reportPath: null,
      },
    });

    expect(screen.getByText(/模组依赖或兼容性有问题/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "复制摘要" })).toBeInTheDocument();
  });

  it("没有摘要时不显示卡片", () => {
    renderView({ selected: "26.3", crash: null });

    expect(screen.queryByRole("button", { name: "复制摘要" })).not.toBeInTheDocument();
  });
});
