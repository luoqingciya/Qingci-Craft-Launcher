import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { SettingsView } from "../api";
import { GameDirectory, InstallProgressBar } from "./GameDirectory";

function settings(overrides: Partial<SettingsView> = {}): SettingsView {
  return {
    gameDir: "C:\\games\\qcl",
    defaultGameDir: "C:\\games\\qcl",
    isCustom: false,
    fallbackReason: null,
    ...overrides,
  };
}

describe("GameDirectory", () => {
  it("显示当前目录，未自定义时禁用「恢复默认」", () => {
    render(
      <GameDirectory settings={settings()} busy={false} onChange={vi.fn()} onReset={vi.fn()} />,
    );

    expect(screen.getByText("C:\\games\\qcl")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "恢复默认" })).toBeDisabled();
    expect(screen.queryByText("自定义")).not.toBeInTheDocument();
  });

  it("自定义目录时显示徽标并允许恢复默认", () => {
    render(
      <GameDirectory
        settings={settings({ gameDir: "D:\\mc", isCustom: true })}
        busy={false}
        onChange={vi.fn()}
        onReset={vi.fn()}
      />,
    );

    expect(screen.getByText("自定义")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "恢复默认" })).toBeEnabled();
  });

  it("回退到其他目录时显示原因", () => {
    // 默认目录不可写时用户必须知道文件实际落在哪，否则会以为下载失败。
    render(
      <GameDirectory
        settings={settings({ fallbackReason: "C:\\Program Files 不可写，已回退到 D:\\data" })}
        busy={false}
        onChange={vi.fn()}
        onReset={vi.fn()}
      />,
    );

    expect(screen.getByText(/已回退到 D:\\data/)).toBeInTheDocument();
  });

  it("读取中时两个按钮都不可点", () => {
    render(<GameDirectory settings={null} busy={false} onChange={vi.fn()} onReset={vi.fn()} />);

    expect(screen.getByText("读取中…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "更改…" })).toBeDisabled();
  });
});

describe("InstallProgressBar", () => {
  it("显示阶段中文名与文件计数", () => {
    render(<InstallProgressBar progress={{ phase: "assets", completed: 515, total: 5147, percent: 10 }} />);

    expect(screen.getByText("正在下载资产 515/5147")).toBeInTheDocument();
    expect(screen.getByText("10%")).toBeInTheDocument();
  });

  it("版本文件阶段用另一个标签", () => {
    render(
      <InstallProgressBar progress={{ phase: "versionFiles", completed: 76, total: 76, percent: 100 }} />,
    );

    expect(screen.getByText("正在下载版本文件 76/76")).toBeInTheDocument();
    expect(screen.getByText("100%")).toBeInTheDocument();
  });
});
