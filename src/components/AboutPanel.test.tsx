// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { UpdateState } from "../lib/useUpdate";
import { AboutPanel } from "./AboutPanel";

const RUNTIME = {
  installed: true,
  msaLogin: true,
  appVersion: "0.1.5",
  tauriVersion: "2.12.5",
  os: "windows",
  arch: "x86_64",
};

function renderPanel(
  update: UpdateState,
  { portable = false, onCheck = vi.fn(), onInstall = vi.fn(), runtime = RUNTIME } = {},
) {
  render(
    <AboutPanel
      update={update}
      portable={portable}
      runtime={runtime}
      onCheck={onCheck}
      onInstall={onInstall}
    />,
  );
  return { onCheck, onInstall };
}

describe("AboutPanel 更新区", () => {
  it("空闲时提示会自动检查，并给出检查按钮", async () => {
    const { onCheck } = renderPanel({ kind: "idle" });

    expect(screen.getByText("启动时会自动检查一次")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "检查更新" }));

    expect(onCheck).toHaveBeenCalled();
  });

  it("检查中按钮不可点", () => {
    renderPanel({ kind: "checking" });

    expect(screen.getByRole("button", { name: "检查中…" })).toBeDisabled();
  });

  it("已是最新版本", () => {
    renderPanel({ kind: "latest" });

    expect(screen.getByText("已是最新版本")).toBeInTheDocument();
  });

  it("发现新版本时显示版本号、说明，并可一键安装", async () => {
    const { onInstall } = renderPanel({
      kind: "available",
      info: { version: "0.1.1", notes: "修复模组页布局" },
    });

    expect(screen.getByText(/发现新版本 v0\.1\.1/)).toBeInTheDocument();
    expect(screen.getByText("修复模组页布局")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "下载并安装" }));
    expect(onInstall).toHaveBeenCalled();
  });

  it("便携版不给自动安装，改成引导去下载页", () => {
    // 便携版替换不了正在运行的 exe；给了按钮只会让人点了没反应。
    renderPanel({ kind: "available", info: { version: "0.1.1", notes: null } }, { portable: true });

    expect(screen.queryByRole("button", { name: "下载并安装" })).toBeNull();
    expect(screen.getByText(/便携版需手动替换/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "去下载" })).toBeInTheDocument();
  });

  it("安装中显示进度条", () => {
    renderPanel({
      kind: "installing",
      info: { version: "0.1.1", notes: null },
      percent: 42,
    });

    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "42");
    expect(screen.getByText("正在下载更新 42%")).toBeInTheDocument();
  });

  it("装完时说明应用会自动重启", () => {
    // 否则用户看到程序自己关掉，会以为崩了。
    renderPanel({
      kind: "installing",
      info: { version: "0.1.1", notes: null },
      percent: 100,
    });

    expect(screen.getByText(/应用会自动重启/)).toBeInTheDocument();
  });

  it("失败的检查显示原因并可重试", async () => {
    const { onCheck } = renderPanel({ kind: "error", message: "网络不可达" });

    expect(screen.getByText("网络不可达")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "重试" }));

    expect(onCheck).toHaveBeenCalled();
  });

  it("不支持的平台不给检查按钮，只引导去下载页", () => {
    // Linux 的安装包不在更新服务器上，给一个「点了就报错」的按钮是最差的选择。
    renderPanel({ kind: "unsupported" });

    expect(screen.getByText("当前平台不支持自动更新")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /检查更新/ })).toBeNull();
    expect(screen.getByRole("link", { name: "去下载" })).toBeInTheDocument();
  });
});

describe("AboutPanel 运行环境", () => {
  it("列出应用版本、系统、Tauri 与 WebView", () => {
    renderPanel({ kind: "idle" });

    // 用 Tauri 版本定位那一行：标题里也有 v0.1.5，按版本号找会撞上两个元素。
    const line = screen.getByText(/Tauri 2\.12\.5/);
    expect(line).toHaveTextContent("v0.1.5");
    expect(line).toHaveTextContent("windows/x86_64");
    expect(line).toHaveTextContent(/WebView/);
  });

  it("读不到运行环境时不崩，明确说明失败", () => {
    // 关于页必须永远能打开——它是用户找「这是什么版本」的地方。
    renderPanel({ kind: "idle" }, { runtime: null });

    expect(screen.getByText("运行环境读取失败")).toBeInTheDocument();
  });

  it("复制的是多行诊断文本", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });

    renderPanel({ kind: "idle" });
    await userEvent.click(screen.getByRole("button", { name: "复制" }));

    await vi.waitFor(() => expect(writeText).toHaveBeenCalled());
    const text = writeText.mock.calls[0][0] as string;
    expect(text).toContain("清辞 v0.1.5");
    expect(text).toContain("系统：windows/x86_64");
    expect(text).toContain("Tauri：2.12.5");
    // 贴进 issue 时要能一行一条，不能挤成一句。
    expect(text.split("\n")).toHaveLength(4);
  });
});
