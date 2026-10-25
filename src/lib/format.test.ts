// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { describe, expect, it } from "vitest";

import { formatDate, phaseLabel, typeLabel } from "./format";

describe("typeLabel", () => {
  it("把官方类型翻译成中文", () => {
    expect(typeLabel("release")).toBe("正式版");
    expect(typeLabel("snapshot")).toBe("快照");
    expect(typeLabel("old_beta")).toBe("Beta");
    expect(typeLabel("old_alpha")).toBe("Alpha");
  });

  it("未知类型原样返回，而不是显示空或 undefined", () => {
    // 上游新增类型时界面仍要能显示点东西。
    expect(typeLabel("new_kind")).toBe("new_kind");
    expect(typeLabel("")).toBe("");
  });
});

describe("formatDate", () => {
  it("把 ISO 时间格式化为本地日期", () => {
    expect(formatDate("2026-09-15T11:23:02+00:00")).toMatch(/2026\/09\/1[45]/);
  });

  it("无法解析时原样返回，不显示 Invalid Date", () => {
    expect(formatDate("not-a-date")).toBe("not-a-date");
    expect(formatDate("")).toBe("");
  });
});

describe("phaseLabel", () => {
  it("区分两个安装阶段", () => {
    expect(phaseLabel("versionFiles")).toBe("版本文件");
    expect(phaseLabel("assets")).toBe("资产");
  });

  it("未知阶段原样返回", () => {
    expect(phaseLabel("somethingElse")).toBe("somethingElse");
  });
});
