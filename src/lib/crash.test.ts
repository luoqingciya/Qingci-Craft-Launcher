// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { describe, expect, it } from "vitest";

import { formatSummary, summarizeCrash } from "./crash";
import type { LogEntry } from "./logs";

let nextId = 1;

/** 把若干行文本变成日志条目。 */
function logs(...texts: string[]): LogEntry[] {
  return texts.map((text) => ({ id: nextId++, stream: "stdout" as const, text }));
}

/** 离线启动必然出现的 401 噪音。 */
const OFFLINE_NOISE = [
  "[12:49:11] [Download-2/ERROR]: Failed to fetch user properties",
  "com.mojang.authlib.exceptions.InvalidCredentialsException: Status: 401",
  "\tat java.base/java.lang.Thread.run(Thread.java:1516)",
  "[12:49:12] [Download-1/ERROR]: Failed to fetch Realms feature flags",
  "com.mojang.realmsclient.exception.RealmsServiceException: Realms authentication error",
  "\tat net.minecraft.client.Minecraft.run(Minecraft.java:1)",
];

describe("summarizeCrash", () => {
  it("正常退出不产生摘要", () => {
    expect(summarizeCrash(logs("游戏退出（0）"), 0)).toBeNull();
  });

  it("被用户终止（没有退出码）不算崩溃", () => {
    expect(summarizeCrash(logs("游戏退出（被终止）"), null)).toBeNull();
  });

  it("只有离线 401 噪音时不把噪音当原因", () => {
    // 这些 401 每次离线启动都有，和崩溃无关。把它们列成「崩溃原因」会误导人。
    const summary = summarizeCrash(logs(...OFFLINE_NOISE), 1);

    expect(summary).not.toBeNull();
    expect(summary?.lines).toEqual([]);
    expect(summary?.headline).toContain("没有可识别的错误");
  });

  it("模组依赖缺失时优先给出模组相关行", () => {
    const summary = summarizeCrash(
      logs(
        ...OFFLINE_NOISE,
        "[12:00:00] [main/ERROR]: Incompatible mod set!",
        "net.fabricmc.loader.impl.FormattedException: Mod resolution encountered an incompatible mod set!",
        "\t- Mod 'Sodium' (sodium) 0.9.3 requires version 1.0.0 or later of fabric-api, which is missing!",
        "java.lang.RuntimeException: something else",
      ),
      1,
    );

    expect(summary?.headline).toContain("模组");
    // 模组行优先，一般异常不该盖过它。
    expect(summary?.lines.some((line) => line.includes("fabric-api"))).toBe(true);
    expect(summary?.lines.every((line) => !line.includes("user properties"))).toBe(true);
  });

  it("内存不足时给出可操作的提示", () => {
    const summary = summarizeCrash(
      logs("java.lang.OutOfMemoryError: Java heap space", "java.lang.RuntimeException: boom"),
      1,
    );

    expect(summary?.headline).toContain("内存不足");
    expect(summary?.lines.some((line) => line.includes("OutOfMemoryError"))).toBe(true);
  });

  it("主类找不到这类启动失败也能识别", () => {
    const summary = summarizeCrash(
      logs("Error: Could not find or load main class net.fabricmc.loader.impl.launch.knot.KnotClient"),
      1,
    );

    expect(summary?.lines.some((line) => line.includes("Could not find or load main class"))).toBe(
      true,
    );
  });

  it("摘要行数有上限，且去重", () => {
    const repeated = Array.from({ length: 30 }, () => "java.lang.RuntimeException: same");
    const summary = summarizeCrash(logs(...repeated), 1);

    expect(summary?.lines).toHaveLength(1);
  });

  it("堆栈帧不进入摘要", () => {
    const summary = summarizeCrash(
      logs("java.lang.RuntimeException: boom", "\tat com.example.Thing.method(Thing.java:42)"),
      1,
    );

    expect(summary?.lines).toHaveLength(1);
    expect(summary?.lines[0]).toContain("boom");
  });

  it("有崩溃报告时用报告里的 Description 作结论", () => {
    // 日志里只有无关噪音，但游戏自己写了报告——结论应该来自报告。
    const summary = summarizeCrash(logs(...OFFLINE_NOISE), 1, {
      path: "/game/crash-reports/crash-1-client.txt",
      description: "Rendering overlay",
      excerpt: ["java.lang.NullPointerException: Cannot invoke \"Foo.bar()\""],
    });

    expect(summary?.headline).toContain("Rendering overlay");
    expect(summary?.lines[0]).toContain("NullPointerException");
    expect(summary?.reportPath).toContain("crash-1-client.txt");
  });

  it("报告里的堆栈排在日志行前面", () => {
    const summary = summarizeCrash(
      logs("java.lang.IllegalStateException: 来自日志"),
      1,
      {
        path: "/game/crash-reports/crash-1-client.txt",
        description: null,
        excerpt: ["java.lang.NullPointerException: 来自报告"],
      },
    );

    expect(summary?.lines[0]).toContain("来自报告");
    expect(summary?.lines[1]).toContain("来自日志");
  });

  it("模组冲突仍然盖过报告里的泛泛描述", () => {
    // 「Rendering overlay」不是可操作的结论，模组缺依赖才是。
    const summary = summarizeCrash(
      logs("Incompatible mod set!"),
      1,
      {
        path: "/game/crash-reports/crash-1-client.txt",
        description: "Rendering overlay",
        excerpt: ["java.lang.RuntimeException: crash"],
      },
    );

    expect(summary?.headline).toContain("模组");
    expect(summary?.lines.some((line) => line.includes("Incompatible mod set"))).toBe(true);
  });

  it("没有报告时 reportPath 为 null", () => {
    const summary = summarizeCrash(logs("java.lang.RuntimeException: boom"), 1);

    expect(summary?.reportPath).toBeNull();
  });
});

describe("formatSummary", () => {
  it("拼出可复制的文本", () => {
    const text = formatSummary({
      exitCode: 1,
      headline: "模组依赖或兼容性有问题，游戏在加载阶段退出",
      lines: ["Incompatible mod set!"],
      reportPath: null,
    });

    expect(text).toContain("退出码 1");
    expect(text).toContain("Incompatible mod set!");
  });

  it("没有关键行时只输出标题", () => {
    const text = formatSummary({
      exitCode: 1,
      headline: "游戏异常退出",
      lines: [],
      reportPath: null,
    });

    expect(text).toBe("游戏异常退出（退出码 1）");
  });

  it("带上报告路径，方便别人按图索骥", () => {
    const text = formatSummary({
      exitCode: 1,
      headline: "游戏崩溃：Rendering overlay",
      lines: [],
      reportPath: "/game/crash-reports/crash-1-client.txt",
    });

    expect(text).toContain("崩溃报告：/game/crash-reports/crash-1-client.txt");
  });
});
