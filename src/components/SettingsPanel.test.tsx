// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { render, screen } from "@testing-library/react";
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

  it("内存超出范围时拒绝保存并提示", async () => {
    const onSave = renderPanel();

    const memoryInput = screen.getByLabelText("最大内存（MB）");
    await userEvent.type(memoryInput, "128");
    await userEvent.click(screen.getByRole("button", { name: "保存" }));

    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByText(/512–65536/)).toBeInTheDocument();
  });

  it("内存填非整数时拒绝保存", async () => {
    const onSave = renderPanel();

    await userEvent.type(screen.getByLabelText("最大内存（MB）"), "4g");
    await userEvent.click(screen.getByRole("button", { name: "保存" }));

    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByText(/必须是整数/)).toBeInTheDocument();
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
      />,
    );

    expect(screen.getByText(/已回退到 D:\\data/)).toBeInTheDocument();
    expect(screen.getByText(/磁盘只读/)).toBeInTheDocument();
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
});
