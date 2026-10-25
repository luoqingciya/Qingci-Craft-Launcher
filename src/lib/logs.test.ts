import { describe, expect, it } from "vitest";

import { MAX_LINES, appendLog, type LogEntry } from "./logs";

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
