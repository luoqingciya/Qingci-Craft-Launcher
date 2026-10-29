// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { describe, expect, it } from "vitest";

import { parseLoaderKind } from "./loader";

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
