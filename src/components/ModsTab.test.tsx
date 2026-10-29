// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ModsTab, formatDownloads } from "./ModsTab";

const HITS = [
  {
    projectId: "AANobbMI",
    title: "Sodium",
    description: "高性能渲染引擎",
    downloads: 123_456_789,
    author: "jellysquid3",
    iconUrl: "https://cdn.modrinth.com/data/AANobbMI/icon.png",
    slug: "sodium",
    zhName: "钠",
  },
  {
    projectId: "gvQqBUqZ",
    title: "Lithium",
    description: "通用性能优化",
    downloads: 45_600,
    author: "jellysquid3",
    iconUrl: null,
    slug: "lithium",
    zhName: null,
  },
];

const searchMods = vi.fn();
const installMod = vi.fn();
const listMods = vi.fn();
const modDependencies = vi.fn();

/** 构造一条已装记录。默认没有来源（模拟手动扔进 mods/ 的 jar）。 */
function installed(fileName: string, projectId: string | null = null) {
  return { fileName, projectId, versionNumber: projectId ? "1.0" : null };
}
const removeMod = vi.fn();
const listModCategories = vi.fn();
const checkModUpdates = vi.fn();
const updateMod = vi.fn();

vi.mock("../api", () => ({
  searchMods: (...args: unknown[]) => searchMods(...args),
  installMod: (...args: unknown[]) => installMod(...args),
  listMods: () => listMods(),
  modDependencies: (...args: unknown[]) => modDependencies(...args),
  removeMod: (...args: unknown[]) => removeMod(...args),
  listModCategories: () => listModCategories(),
  checkModUpdates: (...args: unknown[]) => checkModUpdates(...args),
  updateMod: (...args: unknown[]) => updateMod(...args),
}));

beforeEach(() => {
  searchMods.mockReset();
  installMod.mockReset();
  listMods.mockReset();
  modDependencies.mockReset();
  modDependencies.mockResolvedValue([]);
  removeMod.mockReset();
  listModCategories.mockReset();
  checkModUpdates.mockReset();
  checkModUpdates.mockResolvedValue({ updates: [], incompatible: [] });
  updateMod.mockReset();
  listModCategories.mockResolvedValue([]);
  listMods.mockResolvedValue([]);
});

describe("ModsTab", () => {
  it("原版（没有加载器）时提示先换加载器，而不是给一个装了也没用的入口", () => {
    render(<ModsTab version="26.3" loader="" instance="默认" />);

    expect(screen.getByText(/原版，模组不会被加载/)).toBeInTheDocument();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  });

  it("没选版本时提示先选版本", () => {
    render(<ModsTab version={null} loader="0.20.1" instance="默认" />);

    expect(screen.getByText(/先选择一个版本/)).toBeInTheDocument();
  });

  it("搜索时带上游戏版本与 fabric 分类", async () => {
    searchMods.mockResolvedValue({ query: "sodium", hits: HITS });
    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);

    await userEvent.type(screen.getByRole("textbox"), "sodium{Enter}");

    expect(searchMods).toHaveBeenCalledWith("sodium", "26.3", "fabric", []);
    expect(await screen.findByText("钠")).toBeInTheDocument();
    // 英文标题作为副标题仍然显示，方便对照。
    expect(screen.getByText("Sodium")).toBeInTheDocument();
    expect(screen.getByText("高性能渲染引擎")).toBeInTheDocument();
  });

  it("安装后提示落地路径并刷新已装列表", async () => {
    searchMods.mockResolvedValue({ query: "sodium", hits: HITS });
    installMod.mockResolvedValue(["mods/sodium.jar"]);
    listMods.mockResolvedValueOnce([]).mockResolvedValueOnce([installed("sodium.jar")]);

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.type(screen.getByRole("textbox"), "sodium{Enter}");
    const buttons = await screen.findAllByRole("button", { name: "安装" });
    await userEvent.click(buttons[0]);

    expect(installMod).toHaveBeenCalledWith("AANobbMI", "26.3", "fabric");
    expect(await screen.findByText("已安装到 mods/sodium.jar")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: /已安装/ }));
    expect(await screen.findByText("sodium.jar")).toBeInTheDocument();
  });

  it("搜索失败时显示错误而不是空白", async () => {
    searchMods.mockRejectedValue("网络错误");
    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);

    await userEvent.type(screen.getByRole("textbox"), "x{Enter}");

    expect(await screen.findByText("网络错误")).toBeInTheDocument();
  });

  it("没有结果时给出提示", async () => {
    searchMods.mockResolvedValue({ query: "zzz", hits: [] });
    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);

    await userEvent.type(screen.getByRole("textbox"), "zzz{Enter}");

    expect(await screen.findByText(/没有找到适配该版本的模组/)).toBeInTheDocument();
  });

  it("空查询不发请求", async () => {
    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);

    await userEvent.type(screen.getByRole("textbox"), "{Enter}");

    expect(searchMods).not.toHaveBeenCalled();
  });
});

describe("formatDownloads", () => {
  it("按量级换单位", () => {
    expect(formatDownloads(999)).toBe("999");
    expect(formatDownloads(45_600)).toBe("4.6 万");
    expect(formatDownloads(123_456_789)).toBe("1.2 亿");
  });
});

describe("卸载", () => {
  it("删除是两步的：先确认，再真的删", async () => {
    listMods.mockResolvedValue([installed("sodium.jar")]);
    removeMod.mockResolvedValue(undefined);

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.click(screen.getByRole("tab", { name: /已安装/ }));
    const deleteButton = await screen.findByRole("button", { name: "删除" });

    await userEvent.click(deleteButton);
    // 只点一次不该动文件——误点一次就删掉是灾难。
    expect(removeMod).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "确认删除" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "确认删除" }));
    expect(removeMod).toHaveBeenCalledWith("sodium.jar");
  });

  it("可以取消删除", async () => {
    listMods.mockResolvedValue([installed("sodium.jar")]);
    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.click(screen.getByRole("tab", { name: /已安装/ }));

    await userEvent.click(await screen.findByRole("button", { name: "删除" }));
    await userEvent.click(screen.getByRole("button", { name: "取消" }));

    expect(screen.queryByRole("button", { name: "确认删除" })).not.toBeInTheDocument();
    expect(removeMod).not.toHaveBeenCalled();
  });

  it("删除失败时显示错误", async () => {
    listMods.mockResolvedValue([installed("sodium.jar")]);
    removeMod.mockRejectedValue("文件被占用");

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.click(screen.getByRole("tab", { name: /已安装/ }));
    await userEvent.click(await screen.findByRole("button", { name: "删除" }));
    await userEvent.click(screen.getByRole("button", { name: "确认删除" }));

    expect(await screen.findByText("文件被占用")).toBeInTheDocument();
  });
});

describe("依赖", () => {
  it("装了多个文件时提示带依赖，而不是只报一个路径", async () => {
    searchMods.mockResolvedValue({ query: "sodium", hits: HITS });
    // 主模组 + Fabric API 两个文件。
    installMod.mockResolvedValue(["mods/modmenu.jar", "mods/fabric-api.jar"]);

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.type(screen.getByRole("textbox"), "modmenu{Enter}");
    const buttons = await screen.findAllByRole("button", { name: "安装" });
    await userEvent.click(buttons[0]);

    expect(await screen.findByText("已安装 2 个文件（含依赖）")).toBeInTheDocument();
  });
});

describe("中文译名", () => {
  it("有译名时中文优先，英文作为副标题", async () => {
    searchMods.mockResolvedValue({ query: "sodium", hits: HITS });
    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);

    await userEvent.type(screen.getByRole("textbox"), "sodium{Enter}");

    // 中文名是主标题，英文名退成灰色副标题——两者都要在，方便对照。
    expect(await screen.findByText("钠")).toBeInTheDocument();
    expect(screen.getByText("Sodium")).toBeInTheDocument();
  });

  it("没有译名时只显示英文，不显示空的中文名", async () => {
    searchMods.mockResolvedValue({ query: "sodium", hits: HITS });
    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);

    await userEvent.type(screen.getByRole("textbox"), "lithium{Enter}");

    expect(await screen.findByText("Lithium")).toBeInTheDocument();
  });

  it("提供在 Modrinth 查看原页的入口", async () => {
    searchMods.mockResolvedValue({ query: "sodium", hits: HITS });
    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);

    await userEvent.type(screen.getByRole("textbox"), "sodium{Enter}");

    // 描述保持英文，所以必须有一个能看原页的入口。
    expect(await screen.findAllByRole("button", { name: "在 Modrinth 查看" })).toHaveLength(2);
  });
});

describe("中文搜索与分类筛选", () => {
  it("中文输入被换成英文时给出提示", async () => {
    searchMods.mockResolvedValue({ query: "sodium", hits: HITS });
    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);

    await userEvent.type(screen.getByRole("textbox"), "钠{Enter}");

    // 用户搜的是中文，实际用的是英文关键词——必须说清楚，否则会以为搜错了。
    expect(await screen.findByText(/已按/)).toBeInTheDocument();
    expect(screen.getByText("sodium")).toBeInTheDocument();
  });

  it("输入本身就是英文时不做多余提示", async () => {
    searchMods.mockResolvedValue({ query: "sodium", hits: HITS });
    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);

    await userEvent.type(screen.getByRole("textbox"), "sodium{Enter}");

    expect(await screen.findByText("钠")).toBeInTheDocument();
    expect(screen.queryByText(/已按/)).not.toBeInTheDocument();
  });

  it("分类按钮点选后作为筛选条件传给搜索", async () => {
    listModCategories.mockResolvedValue([
      { id: "optimization", label: "优化" },
      { id: "technology", label: "科技" },
    ]);
    searchMods.mockResolvedValue({ query: "sodium", hits: HITS });

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);

    const chip = await screen.findByRole("button", { name: "优化" });
    await userEvent.click(chip);
    expect(chip).toHaveAttribute("aria-pressed", "true");

    await userEvent.type(screen.getByRole("textbox"), "sodium{Enter}");

    expect(searchMods).toHaveBeenCalledWith("sodium", "26.3", "fabric", ["optimization"]);
  });

  it("再点一次取消该分类", async () => {
    listModCategories.mockResolvedValue([{ id: "optimization", label: "优化" }]);
    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);

    const chip = await screen.findByRole("button", { name: "优化" });
    await userEvent.click(chip);
    await userEvent.click(chip);

    expect(chip).toHaveAttribute("aria-pressed", "false");
  });
});

describe("纯分类浏览", () => {
  it("只选分类、不输关键词也能搜索", async () => {
    listModCategories.mockResolvedValue([{ id: "optimization", label: "优化" }]);
    searchMods.mockResolvedValue({ query: "", hits: HITS });

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);

    const button = screen.getByRole("button", { name: "搜索" });
    expect(button).toBeDisabled();

    await userEvent.click(await screen.findByRole("button", { name: "优化" }));
    expect(button).toBeEnabled();

    await userEvent.click(button);
    expect(searchMods).toHaveBeenCalledWith("", "26.3", "fabric", ["optimization"]);
  });

  it("既没关键词也没分类时不发请求", async () => {
    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);

    expect(screen.getByRole("button", { name: "搜索" })).toBeDisabled();
    expect(searchMods).not.toHaveBeenCalled();
  });
});

describe("更新检查", () => {
  it("没有更新时给出明确结论", async () => {
    listMods.mockResolvedValue([installed("sodium.jar")]);
    checkModUpdates.mockResolvedValue({ updates: [], incompatible: [] });

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.click(screen.getByRole("tab", { name: /已安装/ }));
    await userEvent.click(await screen.findByRole("button", { name: "检查更新" }));

    expect(checkModUpdates).toHaveBeenCalledWith("26.3", "fabric");
    expect(await screen.findByText("所有模组都是最新版本")).toBeInTheDocument();
  });

  it("有新版本时显示版本变化并可更新", async () => {
    listMods.mockResolvedValue([installed("sodium-old.jar")]);
    checkModUpdates.mockResolvedValue({
      updates: [
        {
          fileName: "sodium-old.jar",
          projectId: "AANobbMI",
          currentVersion: "0.9.0",
          latestVersion: "0.9.3",
          latestFileName: "sodium-new.jar",
        },
      ],
      incompatible: [],
    });
    updateMod.mockResolvedValue(["mods/sodium-new.jar"]);

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.click(screen.getByRole("tab", { name: /已安装/ }));
    await userEvent.click(await screen.findByRole("button", { name: "检查更新" }));

    expect(await screen.findByText("0.9.0 → 0.9.3")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "更新" }));

    expect(updateMod).toHaveBeenCalledWith(
      "AANobbMI",
      "sodium-old.jar",
      "26.3",
      "fabric"
    );
    expect(await screen.findByText("已更新到 0.9.3")).toBeInTheDocument();
  });

  it("没有已装模组时不能点检查更新", async () => {
    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.click(screen.getByRole("tab", { name: /已安装/ }));

    expect(screen.getByRole("button", { name: "检查更新" })).toBeDisabled();
  });
});

describe("布局密度", () => {
  it("默认停在搜索页，切到「已安装」才显示列表", async () => {
    listMods.mockResolvedValue([installed("sodium.jar")]);
    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);

    // 两页分开：搜索结果与已装列表不再争同一块高度。
    expect(await screen.findByRole("tab", { name: /已安装 1/ })).toBeInTheDocument();
    expect(screen.queryByText("sodium.jar")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: /已安装/ }));
    expect(screen.getByText("sodium.jar")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("tab", { name: "搜索" }));
    expect(screen.queryByText("sodium.jar")).not.toBeInTheDocument();
  });

  it("有更新时在子标签上标出数量，不用切页也能看到", async () => {
    listMods.mockResolvedValue([installed("sodium-old.jar")]);
    checkModUpdates.mockResolvedValue({
      updates: [
        {
          fileName: "sodium-old.jar",
          projectId: "AANobbMI",
          currentVersion: "0.9.0",
          latestVersion: "0.9.3",
          latestFileName: "sodium-new.jar",
        },
      ],
      incompatible: [],
    });

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.click(screen.getByRole("tab", { name: /已安装/ }));
    await userEvent.click(await screen.findByRole("button", { name: "检查更新" }));

    expect(await screen.findByText("1 可更新")).toBeInTheDocument();
  });

  it("分类默认只露前 8 个，其余收进「更多」", async () => {
    const many = Array.from({ length: 12 }, (_, index) => ({
      id: `cat-${index}`,
      label: `分类${index}`,
    }));
    listModCategories.mockResolvedValue(many);

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);

    expect(await screen.findByRole("button", { name: "分类0" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "分类8" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /更多 4/ }));
    expect(screen.getByRole("button", { name: "分类8" })).toBeInTheDocument();
  });
});

describe("已安装标记来自后端清单", () => {
  it("清单里有项目 id 的结果标成「已安装」，其余仍是「安装」", async () => {
    // 关键点：判断依据是清单里的 projectId，不是文件名。
    // 文件名格式各家都不一样，靠猜必然出错。
    searchMods.mockResolvedValue({ query: "sodium", hits: HITS });
    listMods.mockResolvedValue([installed("sodium-fabric-0.6.13.jar", "AANobbMI")]);

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.type(screen.getByRole("textbox"), "sodium{Enter}");

    expect(await screen.findByRole("button", { name: "已安装" })).toBeDisabled();
    expect(screen.getAllByRole("button", { name: "安装" })).toHaveLength(1);
  });

  it("手动放进 mods/ 的 jar 不影响搜索结果", async () => {
    // 没有 projectId 的条目不该让任何结果变成「已安装」。
    searchMods.mockResolvedValue({ query: "sodium", hits: HITS });
    listMods.mockResolvedValue([installed("whatever.jar")]);

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.type(screen.getByRole("textbox"), "sodium{Enter}");

    expect(await screen.findByText("钠")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "已安装" })).toBeNull();
  });
});

describe("依赖提示", () => {
  it("点「依赖」才去查，并标出已经装了的", async () => {
    searchMods.mockResolvedValue({ query: "sodium", hits: HITS });
    modDependencies.mockResolvedValue([
      { projectId: "P7dR8mSH", title: "Fabric API", installed: true },
      { projectId: "Ha28R6CL", title: null, installed: false },
    ]);

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.type(screen.getByRole("textbox"), "sodium{Enter}");
    await screen.findByText("钠");

    // 列表渲染时不查——每个项目一次请求，塞进搜索会拖慢列表。
    expect(modDependencies).not.toHaveBeenCalled();

    await userEvent.click(screen.getAllByRole("button", { name: "依赖" })[0]);

    expect(await screen.findByText(/Fabric API/)).toBeInTheDocument();
    expect(screen.getByText(/Fabric API · 已装/)).toBeInTheDocument();
    // 标题查不到时退回项目 id，不能空着。
    expect(screen.getByText("Ha28R6CL")).toBeInTheDocument();
    expect(modDependencies).toHaveBeenCalledWith("AANobbMI", "26.3", "fabric");
  });

  it("没有必需依赖时明说，而不是留空", async () => {
    searchMods.mockResolvedValue({ query: "sodium", hits: HITS });
    modDependencies.mockResolvedValue([]);

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.type(screen.getByRole("textbox"), "sodium{Enter}");
    await screen.findByText("钠");

    await userEvent.click(screen.getAllByRole("button", { name: "依赖" })[0]);

    expect(await screen.findByText("没有必需依赖")).toBeInTheDocument();
  });
});

describe("不兼容提示", () => {
  it("检查后列出不适配当前版本的模组", async () => {
    listMods.mockResolvedValue([installed("old-mod.jar", "oldproject"), installed("sodium.jar")]);
    checkModUpdates.mockResolvedValue({
      updates: [],
      incompatible: [{ fileName: "old-mod.jar", projectId: "oldproject" }],
    });

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.click(screen.getByRole("tab", { name: /已安装/ }));
    await userEvent.click(await screen.findByRole("button", { name: "检查更新" }));

    expect(await screen.findByText(/1 个模组不适配 26\.3 · Fabric 0\.20\.1/)).toBeInTheDocument();
    // 汇总块和列表行各出现一次：用户既看得到「有几个」，也看得到「是哪个」。
    expect(screen.getAllByText("old-mod.jar")).toHaveLength(2);
    // 行上也有标记。
    expect(screen.getByText("不适配")).toBeInTheDocument();
  });

  it("全部兼容时不出现警告块", async () => {
    listMods.mockResolvedValue([installed("sodium.jar")]);
    checkModUpdates.mockResolvedValue({ updates: [], incompatible: [] });

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.click(screen.getByRole("tab", { name: /已安装/ }));
    await userEvent.click(await screen.findByRole("button", { name: "检查更新" }));

    expect(await screen.findByText("所有模组都是最新版本")).toBeInTheDocument();
    expect(screen.queryByText(/不适配/)).toBeNull();
  });

  it("有更新又有不兼容时两件事都说", async () => {
    listMods.mockResolvedValue([installed("sodium-old.jar"), installed("old-mod.jar")]);
    checkModUpdates.mockResolvedValue({
      updates: [
        {
          fileName: "sodium-old.jar",
          projectId: "AANobbMI",
          currentVersion: "0.9.0",
          latestVersion: "0.9.3",
          latestFileName: "sodium-new.jar",
        },
      ],
      incompatible: [{ fileName: "old-mod.jar", projectId: "oldproject" }],
    });

    render(<ModsTab version="26.3" loader="0.20.1" instance="默认" />);
    await userEvent.click(screen.getByRole("tab", { name: /已安装/ }));
    await userEvent.click(await screen.findByRole("button", { name: "检查更新" }));

    // 提示条只说更新（它是成功色）；不兼容由上面那块警告单独说。
    expect(await screen.findByText("1 个模组有新版本")).toBeInTheDocument();
    expect(screen.getByText(/1 个模组不适配/)).toBeInTheDocument();
  });
});

describe("切换实例后已装列表", () => {
  it("重新读一次，不会停在切换前的实例上", async () => {
    // `mods/` 每个实例各一份。原来 effect 依赖为空数组，切实例后标题变了、
    // 列表没变——看起来像「把所有实例的模组都列出来了」。
    const { rerender } = render(
      <ModsTab version="26.2" loader="0.20.1" instance="26-2-f" />
    );
    await waitFor(() => expect(listMods).toHaveBeenCalledTimes(1));

    rerender(<ModsTab version="26.2" loader="0.20.1" instance="另一个实例" />);

    await waitFor(() => expect(listMods).toHaveBeenCalledTimes(2));
  });
});
