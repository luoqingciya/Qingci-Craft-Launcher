// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { describe, expect, it } from "vitest";

import {
  MAX_LINES,
  appendLog,
  filterLogs,
  levelOf,
  type LogEntry,
  type LogStreamKind,
} from "./logs";

function rawLine(text: string, stream: LogStreamKind = "stdout"): LogEntry {
  return { id: 1, stream, text };
}

function line(id: number): LogEntry {
  return { id, stream: "stdout", text: `第 ${id} 行` };
}

describe("appendLog", () => {
  it("按顺序追加", () => {
    const result = appendLog([line(1)], line(2));
    expect(result.map((entry) => entry.id)).toEqual([1, 2]);
  });

  it("不修改入参数组", () => {
    // React 靠引用变化判断重渲染，原地 push 会导致界面不更新。
    const original: LogEntry[] = [line(1)];
    const snapshot = [...original];

    appendLog(original, line(2));

    expect(original).toEqual(snapshot);
  });

  it("超过上限时保留最近的若干行", () => {
    const lines = Array.from({ length: 5 }, (_, index) => line(index + 1));

    const result = appendLog(lines, line(6), 3);

    expect(result.map((entry) => entry.id)).toEqual([4, 5, 6]);
  });

  it("上限为 0 时不保留任何行", () => {
    expect(appendLog([line(1)], line(2), 0)).toEqual([]);
  });

  it("默认上限足够大，正常会话不会被截断", () => {
    expect(MAX_LINES).toBeGreaterThanOrEqual(1000);

    const lines = Array.from({ length: 10 }, (_, index) => line(index));
    expect(appendLog(lines, line(99))).toHaveLength(11);
  });
});

describe("日志级别与筛选", () => {
  it("认得出 Minecraft 的级别标记", () => {
    expect(levelOf(rawLine("[12:04:34] [Render thread/WARN]: 慢"))).toBe("warn");
    expect(levelOf(rawLine("[12:04:34] [Render thread/ERROR]: 炸了"))).toBe("error");
    expect(levelOf(rawLine("[12:04:34] [main/INFO]: 正常"))).toBe("info");
  });

  it("没有标记时按关键词判断", () => {
    expect(levelOf(rawLine("java.lang.NullPointerException: ERROR while loading"))).toBe("error");
    expect(levelOf(rawLine("Warning: 这个模组有点旧"))).toBe("warn");
    // 词边界：不能把含 error 的普通单词当错误
    expect(levelOf(rawLine("terror of the deep"))).toBe("info");
  });

  it("stderr 没写级别时算错误，system 一律算信息", () => {
    expect(levelOf(rawLine("随便什么", "stderr"))).toBe("error");
    expect(levelOf(rawLine("随便什么", "system"))).toBe("info");
  });

  it("带级别标记的 stderr 行按标记走", () => {
    // 有些 WARN 走 stderr。先按流判断会把它误判成错误，点「警告」就一行都筛不出来。
    expect(levelOf(rawLine("[1] [Render thread/WARN]: 慢", "stderr"))).toBe("warn");
  });

  it("按级别筛选", () => {
    const lines = [
      rawLine("[1] [main/INFO]: a"),
      rawLine("[2] [main/WARN]: b"),
      rawLine("[3] [main/ERROR]: c"),
    ];
    expect(filterLogs(lines, "warn", "").map((l) => l.text)).toEqual(["[2] [main/WARN]: b"]);
    expect(filterLogs(lines, "all", "").length).toBe(3);
  });

  it("按关键词筛选，忽略大小写与首尾空白", () => {
    const lines = [rawLine("Loading 63 mods"), rawLine("Sound engine started")];
    expect(filterLogs(lines, "all", "  MODS ").map((l) => l.text)).toEqual(["Loading 63 mods"]);
  });

  it("级别与关键词同时生效", () => {
    const lines = [rawLine("[1] [main/WARN]: mods 有问题"), rawLine("[2] [main/INFO]: mods 正常")];
    expect(filterLogs(lines, "warn", "mods").length).toBe(1);
    expect(filterLogs(lines, "error", "mods").length).toBe(0);
  });
});
