// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { describe, expect, it } from "vitest";

import { isLoaderVersionRejected, parseLoaderKind } from "./loader";

describe("parseLoaderKind", () => {
  it("认出 Forge 与 NeoForge", () => {
    expect(parseLoaderKind("forge")).toBe("forge");
    expect(parseLoaderKind("neoforge")).toBe("neoforge");
  });

  it("旧实例没有这个字段时按 Fabric 处理", () => {
    // 加这个字段之前只有 Fabric，所以 null 等价于 Fabric。
    expect(parseLoaderKind(null)).toBe("fabric");
    expect(parseLoaderKind(undefined)).toBe("fabric");
  });

  it("认不出来的值退回 Fabric，而不是让界面拿到不存在的类型", () => {
    expect(parseLoaderKind("quilt")).toBe("fabric");
    expect(parseLoaderKind("")).toBe("fabric");
  });
});

describe("isLoaderVersionRejected", () => {
  it("上游 404 拒绝加载器版本时算", () => {
    // 真实文案：Forge 实例上拿了 Fabric 的版本号。
    const message =
      "安装 NeoForge 失败：获取失败：HTTP 404";
    expect(isLoaderVersionRejected(message)).toBe(true);
  });

  it("Fabric profile 400 也算", () => {
    const message =
      "获取 Fabric profile 失败：获取失败：HTTP 400（如果这个版本号是 Forge / NeoForge 的，说明加载器类型没对上）";
    expect(isLoaderVersionRejected(message)).toBe(true);
  });

  it("网络错误不算——重试就好，不该清掉用户的选择", () => {
    const message =
      "安装 Forge 失败：获取失败：网络错误：error decoding response body for url (https://…)";
    expect(isLoaderVersionRejected(message)).toBe(false);
  });

  it("和加载器无关的错误不算", () => {
    expect(isLoaderVersionRejected("下载资产失败：HTTP 404")).toBe(false);
  });
});
