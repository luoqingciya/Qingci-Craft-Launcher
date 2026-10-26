// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { beforeEach, describe, expect, it, vi } from "vitest";

import { checkForUpdate, updateSupported } from "./update";

const check = vi.fn();

vi.mock("@tauri-apps/plugin-updater", () => ({
  check: () => check(),
}));

/** 造一个插件 Update 对象。 */
function fakeUpdate(overrides: { version?: string; body?: string } = {}) {
  const close = vi.fn().mockResolvedValue(undefined);
  const downloadAndInstall = vi.fn().mockImplementation(async (onEvent) => {
    onEvent({ event: "Started", data: { contentLength: 1000 } });
    onEvent({ event: "Progress", data: { chunkLength: 250 } });
    onEvent({ event: "Progress", data: { chunkLength: 250 } });
    onEvent({ event: "Finished" });
  });
  return {
    update: {
      version: overrides.version ?? "0.1.1",
      body: overrides.body,
      close,
      downloadAndInstall,
    },
    close,
    downloadAndInstall,
  };
}

beforeEach(() => {
  check.mockReset();
});

describe("checkForUpdate", () => {
  it("没有更新时返回 null", async () => {
    check.mockResolvedValue(null);

    await expect(checkForUpdate()).resolves.toBeNull();
  });

  it("把插件的 Update 翻译成版本号与更新说明", async () => {
    const { update } = fakeUpdate({ version: "0.2.0", body: "修了几个问题" });
    check.mockResolvedValue(update);

    const pending = await checkForUpdate();

    expect(pending?.info).toEqual({ version: "0.2.0", notes: "修了几个问题" });
  });

  it("没有 notes 时给 null，不是空字符串", async () => {
    // 空字符串会让界面渲染出一个空段落。
    const { update } = fakeUpdate();
    check.mockResolvedValue(update);

    expect((await checkForUpdate())?.info.notes).toBeNull();
  });

  it("按已下载字节数算百分比", async () => {
    const { update } = fakeUpdate();
    check.mockResolvedValue(update);

    const pending = await checkForUpdate();
    const percents: number[] = [];
    await pending?.install((percent) => percents.push(percent));

    expect(percents).toEqual([25, 50, 100]);
  });

  it("服务端没给 Content-Length 时不乱报百分比", async () => {
    // 算不出百分比就停在 0，比让进度条乱跳诚实。
    const update = {
      version: "0.1.1",
      body: undefined,
      close: vi.fn(),
      downloadAndInstall: vi.fn().mockImplementation(async (onEvent) => {
        onEvent({ event: "Started", data: {} });
        onEvent({ event: "Progress", data: { chunkLength: 999 } });
      }),
    };
    check.mockResolvedValue(update);

    const pending = await checkForUpdate();
    const percents: number[] = [];
    await pending?.install((percent) => percents.push(percent));

    expect(percents).toEqual([]);
  });

  it("discard 会释放插件资源", async () => {
    const { update, close } = fakeUpdate();
    check.mockResolvedValue(update);

    await (await checkForUpdate())?.discard();

    expect(close).toHaveBeenCalledTimes(1);
  });
});

describe("updateSupported", () => {
  it("浏览器里（没有 Tauri 运行时）判定为不支持", () => {
    expect(updateSupported()).toBe(false);
  });

  it("有 Tauri 运行时则支持", () => {
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    try {
      expect(updateSupported()).toBe(true);
    } finally {
      Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
    }
  });
});
