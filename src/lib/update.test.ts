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

/**
 * 在「有 Tauri 运行时 + 指定 UA」的环境里跑断言。
 *
 * **必须显式给 UA**：jsdom 的默认 UA 是跟着宿主平台走的——Linux 的 runner 上
 * 它本身就含 `X11; Linux x86_64`，不写死就会得到「本机通过、Linux 失败」。
 */
function withRuntime(userAgent: string, run: () => void) {
  Object.assign(window, { __TAURI_INTERNALS__: {} });
  const original = navigator.userAgent;
  Object.defineProperty(window.navigator, "userAgent", { value: userAgent, configurable: true });
  try {
    run();
  } finally {
    Object.defineProperty(window.navigator, "userAgent", {
      value: original,
      configurable: true,
    });
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  }
}

const WINDOWS_UA =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko)";
const LINUX_UA = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)";

describe("updateSupported", () => {
  it("浏览器里（没有 Tauri 运行时）判定为不支持", () => {
    expect(updateSupported()).toBe(false);
  });

  it("Windows 上有 Tauri 运行时则支持", () => {
    withRuntime(WINDOWS_UA, () => {
      expect(updateSupported()).toBe(true);
    });
  });

  it("Linux 上判定为不支持——清单里没有对应平台", () => {
    // AppImage 有 80 MB，不上 COS，只在 GitHub Release 提供，所以没有可下载的更新地址。
    withRuntime(LINUX_UA, () => {
      expect(updateSupported()).toBe(false);
    });
  });
});
