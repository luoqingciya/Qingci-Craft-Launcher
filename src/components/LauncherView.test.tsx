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
  selectedVersion: null,
  loaderVersion: null,
  loaderKind: null,
  downloadSource: "official",
  instances: [{ name: "默认", gameDir: "C:\\games\\qcl", isDefaultDir: false, active: true, selectedVersion: null, loaderVersion: null, loaderKind: null }],
  activeInstance: "默认",
};

function renderView(overrides: Partial<Parameters<typeof LauncherView>[0]> = {}) {
  const props = {
    versions: { status: "ready" as const, versions: VERSIONS },
    selected: null as string | null,
    loaders: [
      { version: "0.20.1", stable: true },
      { version: "0.21.0-beta.1", stable: false },
    ],
    loaderKind: "fabric" as const,
    onLoaderKindChange: vi.fn(),
    loader: "",
    loaderBusy: false,
    busy: false,
    progress: null,
    status: { running: false, version: null },
    logs: [],
    crash: null,
    account: null,
    restoring: false,
    msaLogin: false,
    loginPhase: { kind: "idle" as const },
    update: { kind: "idle" as const },
    portable: false,
    runtime: null,
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
    onSwitchInstance: vi.fn(),
    onCreateInstance: vi.fn(),
    onRenameInstance: vi.fn(),
    onDeleteInstance: vi.fn(),
    modCount: 63,
    onOpenGameDir: vi.fn(),
    onRefreshModCount: vi.fn(),
    onDismissMessage: vi.fn(),
    onDismissError: vi.fn(),
    ...overrides,
  };
  render(<LauncherView {...props} />);
  return props;
}

describe("LauncherView", () => {
  it("下载界面标注 BMCLAPI 来源（走镜像时）", () => {
    renderView({
      settings: { ...SETTINGS, downloadSource: "bmclapi" },
      progress: { phase: "assets", completed: 1, total: 10, percent: 10 },
    });

    expect(screen.getByRole("link", { name: "BMCLAPI" })).toBeInTheDocument();
  });

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

  it("加载器分两步：先选类型，再选版本", async () => {
    const props = renderView({ selected: "26.3" });

    await userEvent.click(screen.getByRole("button", { name: "更换加载器" }));

    // 第一步只有类型，不混入版本号——挤在一个列表里读不出层次。
    const kinds = within(screen.getByRole("dialog"));
    expect(kinds.getByText("选择加载器类型")).toBeInTheDocument();
    expect(kinds.getByText("无（原版）")).toBeInTheDocument();
    expect(kinds.getByText("Forge")).toBeInTheDocument();
    expect(kinds.queryByText("0.20.1")).not.toBeInTheDocument();

    await userEvent.click(kinds.getByText("Fabric"));

    // 第二步才是版本列表。
    const versions = within(screen.getByRole("dialog"));
    expect(versions.getByText("0.20.1")).toBeInTheDocument();
    expect(versions.getByText("测试版")).toBeInTheDocument();

    await userEvent.click(versions.getByText("0.20.1"));
    expect(props.onLoaderChange).toHaveBeenCalledWith("0.20.1");
  });

  it("选「无（原版）」时清空加载器", async () => {
    const props = renderView({ selected: "26.3", loader: "" });

    await userEvent.click(screen.getByRole("button", { name: "更换加载器" }));
    await userEvent.click(
      within(screen.getByRole("dialog")).getByText("无（原版）")
    );

    expect(props.onLoaderChange).toHaveBeenCalledWith("");
  });

  it("选中加载器后主按钮副标题带上 Fabric", () => {
    renderView({ selected: "26.3", loader: "0.20.1" });

    expect(screen.getByText("26.3 · Fabric 0.20.1")).toBeInTheDocument();
  });

  it("未选版本时加载器入口禁用", () => {
    renderView();

    expect(screen.getByRole("button", { name: "更换加载器" })).toBeDisabled();
  });

  it("选定版本后版本行锁定，不再是可点入口", () => {
    // 换版本会让这个实例里已装的模组与文件对不上，所以选定即固定。
    renderView({ selected: "26.3" });

    expect(
      screen.queryByRole("button", { name: "更换版本" })
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "版本已固定" })
    ).toBeInTheDocument();
  });

  it("装了加载器后加载器行也锁定", () => {
    renderView({ selected: "26.3", loader: "0.20.1" });

    expect(
      screen.getByRole("button", { name: "加载器已固定" })
    ).toBeInTheDocument();
  });

  it("启动页可以直接新建实例，不用绕去设置页", async () => {
    const props = renderView();

    await userEvent.click(screen.getByRole("button", { name: "新建实例" }));
    await userEvent.type(screen.getByLabelText("新实例名"), "冒险");
    await userEvent.click(screen.getByRole("button", { name: "创建" }));

    expect(props.onCreateInstance).toHaveBeenCalledWith("冒险");
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

describe("实例切换器", () => {
  const TWO = [
    {
      name: "默认",
      gameDir: "C:\\games\\qcl",
      isDefaultDir: false,
      active: true,
      selectedVersion: null,
      loaderVersion: null,
      loaderKind: null,
    },
    {
      name: "整合包",
      gameDir: "C:\\games\\pack",
      isDefaultDir: false,
      active: false,
      selectedVersion: null,
      loaderVersion: null,
      loaderKind: null,
    },
  ];

  it("只有一个实例时不占地方", () => {
    // 只有一个实例时切换器没有任何可选项，多一行只是噪音。
    renderView();

    expect(screen.queryByLabelText("切换实例")).toBeNull();
  });

  it("多个实例时出现在启动面板上", async () => {
    const onSwitch = vi.fn();
    renderView({
      settings: { ...SETTINGS, instances: TWO, activeInstance: "默认" },
      onSwitchInstance: onSwitch,
    });

    const select = screen.getByLabelText("切换实例");
    expect(select).toHaveValue("默认");

    await userEvent.selectOptions(select, "整合包");

    expect(onSwitch).toHaveBeenCalledWith("整合包");
  });
});

describe("启动页信息区", () => {
  it("显示游戏目录与已装模组数", () => {
    renderView({ modCount: 63 });

    expect(screen.getByText(/已装模组/)).toBeInTheDocument();
    expect(screen.getByText("63 个")).toBeInTheDocument();
    // 目录用等宽字体截断显示，完整路径放在 title 里。
    expect(screen.getByTitle(SETTINGS.gameDir)).toBeInTheDocument();
  });

  it("模组数没读出来时不显示 0", () => {
    // 0 会被读成「一个都没装」，那是错误信息。
    renderView({ modCount: null });

    expect(screen.getByText("读取中…")).toBeInTheDocument();
  });

  it("点目录会用系统文件管理器打开", async () => {
    const onOpen = vi.fn();
    renderView({ onOpenGameDir: onOpen });

    await userEvent.click(screen.getByTitle(SETTINGS.gameDir));

    expect(onOpen).toHaveBeenCalled();
  });
});

describe("账号恢复中", () => {
  it("恢复期间说「正在恢复登录」，不说「离线账户」", () => {
    // 恢复要花一到几秒。这段时间说「离线账户」，等刷新回来再跳成「正版账户」，
    // 用户看到的就是账号时有时无。
    renderView({ restoring: true, account: null, msaLogin: true });

    expect(screen.getByText("正在恢复登录…")).toBeInTheDocument();
    expect(screen.queryByText("离线账户")).toBeNull();
  });

  it("恢复期间不显示登录/退出按钮", () => {
    // 还不知道要不要登录，给个按钮只会让人点错。
    renderView({ restoring: true, account: null, msaLogin: true });

    expect(screen.queryByRole("button", { name: "登录" })).toBeNull();
    expect(screen.queryByRole("button", { name: "退出" })).toBeNull();
  });

  it("恢复结束后没有账号才说「离线账户」并给出登录入口", () => {
    renderView({ restoring: false, account: null, msaLogin: true });

    expect(screen.getByText("离线账户")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "登录" })).toBeInTheDocument();
  });

  it("恢复成功则显示账号名与正版账户", () => {
    renderView({
      restoring: false,
      account: { name: "Steve", uuid: "069a79f444e94726a5befca90e38aaf5" },
      msaLogin: true,
    });

    expect(screen.getByText("正版账户")).toBeInTheDocument();
    expect(screen.getByText("Steve")).toBeInTheDocument();
  });
});
