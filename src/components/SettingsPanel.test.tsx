// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { JavaOption, SettingsView } from "../api";
import { InstallProgressBar, SettingsPanel } from "./SettingsPanel";

function settings(overrides: Partial<SettingsView> = {}): SettingsView {
  return {
    gameDir: "C:\\games\\qcl",
    defaultGameDir: "C:\\games\\qcl",
    isCustom: false,
    fallbackReason: null,
    playerName: "Player",
    memoryMb: null,
    javaPath: null,
    memoryRange: [512, 65536],
    selectedVersion: null,
    loaderVersion: null,
    downloadSource: "official",
    instances: [{ name: "默认", gameDir: "C:\\games\\qcl", isDefaultDir: false, active: true, selectedVersion: null, loaderVersion: null, loaderKind: null }],
    activeInstance: "默认",
    ...overrides,
  };
}

const JAVA: JavaOption[] = [
  { path: "C:\\jdk-26\\bin\\java.exe", version: "26.0.2", major: 26, source: "JAVA_HOME" },
  { path: "C:\\jre-21\\bin\\java.exe", version: "21.0.12", major: 21, source: "系统安装目录" },
];

function renderPanel(overrides: Partial<SettingsView> = {}, onSave = vi.fn()) {
  render(
    <SettingsPanel
      settings={settings(overrides)}
      javaOptions={JAVA}
      saving={false}
      error={null}
      onSave={onSave}
      onRefreshJava={vi.fn()}
      onSwitchInstance={vi.fn()}
      onCreateInstance={vi.fn()}
      onRenameInstance={vi.fn()}
      onDeleteInstance={vi.fn()}
    />,
  );
  return onSave;
}

describe("SettingsPanel", () => {
  it("显示当前设置值", () => {
    renderPanel({ memoryMb: 4096 });

    expect(screen.getByDisplayValue("C:\\games\\qcl")).toBeInTheDocument();
    expect(screen.getByDisplayValue("Player")).toBeInTheDocument();
    expect(screen.getByDisplayValue("4096")).toBeInTheDocument();
  });

  it("未改动时「保存」与「撤销」都禁用", () => {
    renderPanel();

    expect(screen.getByRole("button", { name: "保存" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "撤销" })).toBeDisabled();
  });

  it("改动后可以保存，提交的是去空白后的值", async () => {
    const onSave = renderPanel();

    const nameInput = screen.getByLabelText("离线玩家名");
    await userEvent.clear(nameInput);
    await userEvent.type(nameInput, "  清辞  ");

    const saveButton = screen.getByRole("button", { name: "保存" });
    expect(saveButton).toBeEnabled();
    await userEvent.click(saveButton);

    expect(onSave).toHaveBeenCalledWith({
      gameDir: "C:\\games\\qcl",
      playerName: "清辞",
      memoryMb: null,
      javaPath: null,
      downloadSource: null,
      installerProxy: null,
    });
  });

  it("撤销回到原值", async () => {
    renderPanel();

    const nameInput = screen.getByLabelText("离线玩家名");
    await userEvent.clear(nameInput);
    await userEvent.type(nameInput, "别的名字");
    expect(screen.getByDisplayValue("别的名字")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "撤销" }));

    expect(screen.getByDisplayValue("Player")).toBeInTheDocument();
  });

  it("滑块调整内存后保存新值", async () => {
    const onSave = renderPanel({ memoryMb: 4096 });

    // range 输入没法用键盘逐字输入，直接派发 change。
    fireEvent.change(screen.getByLabelText("最大内存"), { target: { value: "6144" } });
    await userEvent.click(screen.getByRole("button", { name: "保存" }));

    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ memoryMb: 6144 }));
  });

  it("未指定内存时滑块禁用并显示「官方默认」", () => {
    renderPanel({ memoryMb: null });

    const slider = screen.getByLabelText("最大内存") as HTMLInputElement;
    expect(slider).toBeDisabled();
    // 停在下限之上只是显示位置，不代表已经指定了值。
    expect(slider.value).toBe("2048");
    expect(screen.getByText("官方默认")).toBeInTheDocument();
  });

  it("取消「跟随官方默认」后滑块可用，并从 2 GB 起", async () => {
    renderPanel({ memoryMb: null });

    await userEvent.click(screen.getByLabelText("跟随官方默认"));

    const slider = screen.getByLabelText("最大内存") as HTMLInputElement;
    expect(slider).not.toBeDisabled();
    expect(slider.value).toBe("2048");
  });

  it("勾上「跟随官方默认」时提交 null", async () => {
    const onSave = renderPanel({ memoryMb: 4096 });

    await userEvent.click(screen.getByLabelText("跟随官方默认"));
    await userEvent.click(screen.getByRole("button", { name: "保存" }));

    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ memoryMb: null }));
  });

  it("玩家名为空时拒绝保存", async () => {
    const onSave = renderPanel();

    await userEvent.clear(screen.getByLabelText("离线玩家名"));
    await userEvent.click(screen.getByRole("button", { name: "保存" }));

    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByText("玩家名不能为空")).toBeInTheDocument();
  });

  it("Java 下拉包含「自动」与已探测到的安装", () => {
    renderPanel();

    const select = screen.getByLabelText("Java 运行时");
    expect(select).toHaveValue("");
    expect(screen.getByRole("option", { name: "自动选择（按版本要求）" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /26\.0\.2/ })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /21\.0\.12/ })).toBeInTheDocument();
  });

  it("选择 Java 后保存会提交路径", async () => {
    const onSave = renderPanel();

    await userEvent.selectOptions(screen.getByLabelText("Java 运行时"), JAVA[0].path);
    await userEvent.click(screen.getByRole("button", { name: "保存" }));

    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ javaPath: JAVA[0].path }));
  });

  it("显示回退原因与外部错误", () => {
    render(
      <SettingsPanel
        settings={settings({ fallbackReason: "C:\\Program Files 不可写，已回退到 D:\\data" })}
        javaOptions={[]}
        saving={false}
        error="保存失败：磁盘只读"
        onSave={vi.fn()}
        onRefreshJava={vi.fn()}
        onSwitchInstance={vi.fn()}
        onCreateInstance={vi.fn()}
        onRenameInstance={vi.fn()}
        onDeleteInstance={vi.fn()}
      />,
    );

    expect(screen.getByText(/已回退到 D:\\data/)).toBeInTheDocument();
    expect(screen.getByText(/磁盘只读/)).toBeInTheDocument();
  });

  it("下载源默认是官方，选中镜像后提交 bmclapi", async () => {
    const onSave = renderPanel();

    const select = screen.getByLabelText("下载源");
    expect(select).toHaveValue("official");

    await userEvent.selectOptions(select, "bmclapi");
    await userEvent.click(screen.getByRole("button", { name: "保存" }));

    // 只有非默认值才进配置：默认值写进文件会让「改回默认」变得看不出来。
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ downloadSource: "bmclapi" }));
  });

  it("选官方时提交 null 而不是字符串", async () => {
    const onSave = renderPanel({ downloadSource: "bmclapi" });

    await userEvent.selectOptions(screen.getByLabelText("下载源"), "official");
    await userEvent.click(screen.getByRole("button", { name: "保存" }));

    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ downloadSource: null }));
  });

  it("只在选中镜像时才标注 BMCLAPI 来源", async () => {
    // BMCLAPI 的使用条款要求在使用处标注来源。
    renderPanel();
    expect(screen.queryByRole("link", { name: "BMCLAPI" })).toBeNull();

    await userEvent.selectOptions(screen.getByLabelText("下载源"), "bmclapi");
    expect(screen.getByRole("link", { name: "BMCLAPI" })).toBeInTheDocument();
  });
});

describe("InstallProgressBar", () => {
  it("显示阶段中文名与文件计数", () => {
    render(
      <InstallProgressBar progress={{ phase: "assets", completed: 515, total: 5147, percent: 10 }} />,
    );

    expect(screen.getByText("正在下载资产 515/5147")).toBeInTheDocument();
    expect(screen.getByText("10%")).toBeInTheDocument();
  });

  it("版本文件阶段用另一个标签", () => {
    render(
      <InstallProgressBar
        progress={{ phase: "versionFiles", completed: 76, total: 76, percent: 100 }}
      />,
    );

    expect(screen.getByText("正在下载版本文件 76/76")).toBeInTheDocument();
    expect(screen.getByText("100%")).toBeInTheDocument();
  });

  it("加载器阶段不是「下载」，按步骤数计", () => {
    // processor 是本地执行的独立 JVM，说「正在下载加载器」会误导。
    render(
      <InstallProgressBar
        progress={{ phase: "loader", completed: 3, total: 6, percent: 50 }}
      />,
    );

    expect(screen.getByText("正在安装加载器 3/6")).toBeInTheDocument();
    expect(screen.getByText("50%")).toBeInTheDocument();
  });
});

describe("InstallProgressBar 的来源标注", () => {
  const progress = { phase: "assets" as const, completed: 10, total: 100, percent: 10 };

  it("走镜像时在下载界面标出 BMCLAPI", () => {
    // BMCLAPI 使用协议第 3 条：使用它的程序必须在下载界面或其他可视部分标明来源。
    render(<InstallProgressBar progress={progress} mirrored />);

    expect(screen.getByRole("link", { name: "BMCLAPI" })).toBeInTheDocument();
  });

  it("官方源不标", () => {
    render(<InstallProgressBar progress={progress} />);

    expect(screen.queryByRole("link", { name: "BMCLAPI" })).toBeNull();
  });
});

describe("实例管理", () => {
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

  it("列出全部实例并标出当前那个", () => {
    renderPanel({ instances: TWO, activeInstance: "默认" });

    expect(screen.getByText("默认")).toBeInTheDocument();
    expect(screen.getByText("整合包")).toBeInTheDocument();
    expect(screen.getByText("当前")).toBeInTheDocument();
  });

  it("点击另一个实例就切换过去", async () => {
    const onSwitch = vi.fn();
    render(
      <SettingsPanel
        settings={settings({ instances: TWO, activeInstance: "默认" })}
        javaOptions={[]}
        saving={false}
        error={null}
        onSave={vi.fn()}
        onRefreshJava={vi.fn()}
        onSwitchInstance={onSwitch}
        onCreateInstance={vi.fn()}
        onRenameInstance={vi.fn()}
        onDeleteInstance={vi.fn()}
      />,
    );

    await userEvent.click(screen.getByText("整合包"));

    expect(onSwitch).toHaveBeenCalledWith("整合包");
  });

  it("新建实例会去掉首尾空白", async () => {
    const onCreate = vi.fn();
    render(
      <SettingsPanel
        settings={settings({ instances: TWO, activeInstance: "默认" })}
        javaOptions={[]}
        saving={false}
        error={null}
        onSave={vi.fn()}
        onRefreshJava={vi.fn()}
        onSwitchInstance={vi.fn()}
        onCreateInstance={onCreate}
        onRenameInstance={vi.fn()}
        onDeleteInstance={vi.fn()}
      />,
    );

    await userEvent.type(screen.getByLabelText("新实例名"), "  生存  ");
    await userEvent.click(screen.getByRole("button", { name: "新建" }));

    expect(onCreate).toHaveBeenCalledWith("生存");
  });

  it("只剩一个实例时不能移除", () => {
    // 删到一个不剩会让界面没有可用的目录，后端也会拒绝——按钮直接禁用更早。
    renderPanel({ instances: [TWO[0]], activeInstance: "默认" });

    expect(screen.getByRole("button", { name: "移除" })).toBeDisabled();
  });

  it("重命名会提交新名字", async () => {
    const onRename = vi.fn();
    render(
      <SettingsPanel
        settings={settings({ instances: TWO, activeInstance: "默认" })}
        javaOptions={[]}
        saving={false}
        error={null}
        onSave={vi.fn()}
        onRefreshJava={vi.fn()}
        onSwitchInstance={vi.fn()}
        onCreateInstance={vi.fn()}
        onRenameInstance={onRename}
        onDeleteInstance={vi.fn()}
      />,
    );

    await userEvent.click(screen.getAllByRole("button", { name: "重命名" })[1]);
    const input = screen.getByLabelText("重命名后的实例名");
    await userEvent.clear(input);
    await userEvent.type(input, "模组服");
    await userEvent.click(screen.getByRole("button", { name: "确定" }));

    expect(onRename).toHaveBeenCalledWith("整合包", "模组服");
  });
});
