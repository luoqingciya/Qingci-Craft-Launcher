// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

//! 界面预览（仅开发用）。
//!
//! `LauncherView` 是纯展示组件，所以可以脱离 Tauri 用假数据渲染：
//! 调布局与配色时不必起桌面壳、也不必真的下载一个版本。
//!
//! ```bash
//! pnpm dev   # 打开 http://localhost:1420/preview.html
//! ```

import { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";

import type { GameStatus, LoaderOption, SettingsView, VersionSummary } from "./api";
import { LauncherView } from "./components/LauncherView";
import type { LogEntry } from "./lib/logs";
import { applyInitialTheme } from "./lib/theme";
import "./styles.css";

/**
 * 最小 IPC 桩。
 *
 * 模组页会真的发 IPC 请求。预览页跑在浏览器里、没有 Tauri，不塞这层的话
 * 切到「模组」只会看到一片错误态——调样式时毫无参考价值。
 *
 * 只覆盖模组页用到的几个命令；其余返回 `null`，调用方自己会落到错误分支。
 */
const MOCK_CATEGORIES = [
  { id: "optimization", label: "性能优化" },
  { id: "decoration", label: "装饰" },
  { id: "library", label: "前置库" },
  { id: "adventure", label: "冒险" },
  { id: "equipment", label: "装备" },
  { id: "technology", label: "科技" },
];

const MOCK_HITS = [
  {
    projectId: "sodium",
    title: "Sodium",
    description: "现代化的渲染优化模组，大幅提升帧率并减少卡顿。",
    downloads: 41_820_553,
    author: "jellysquid3",
    iconUrl: null,
    slug: "sodium",
    zhName: "钠",
  },
  {
    projectId: "iris",
    title: "Iris Shaders",
    description: "兼容 OptiFine 光影包的高性能光影加载器。",
    downloads: 18_204_991,
    author: "coderbot",
    iconUrl: null,
    slug: "iris",
    zhName: "鸢尾花光影",
  },
  {
    projectId: "lithium",
    title: "Lithium",
    description: "不改变原版行为的前提下优化游戏逻辑与服务端性能。",
    downloads: 12_663_410,
    author: "jellysquid3",
    iconUrl: null,
    slug: "lithium",
    zhName: "锂",
  },
  {
    projectId: "fabric-api",
    title: "Fabric API",
    description: "绝大多数 Fabric 模组依赖的前置库。",
    downloads: 63_114_882,
    author: "modmuss50",
    iconUrl: null,
    slug: "fabric-api",
    zhName: "Fabric API",
  },
];

if (!("__TAURI_INTERNALS__" in window)) {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {
      invoke: async (command: string) => {
        switch (command) {
          case "list_mods":
            return ["fabric-api-0.140.2+26.3.jar", "sodium-fabric-0.6.13.jar"];
          case "list_mod_categories":
            return MOCK_CATEGORIES;
          case "search_mods":
            return { query: "优化", hits: MOCK_HITS };
          case "check_mod_updates":
            return [];
          default:
            return null;
        }
      },
    },
  });
}

applyInitialTheme();

const VERSIONS: VersionSummary[] = [
  { id: "26.4-snapshot-2", type: "snapshot", releaseTime: "2026-10-22T10:00:00+00:00" },
  { id: "26.3", type: "release", releaseTime: "2026-09-15T11:23:02+00:00" },
  { id: "26.3-rc-1", type: "snapshot", releaseTime: "2026-09-10T08:12:00+00:00" },
  { id: "26.2", type: "release", releaseTime: "2026-08-05T09:30:00+00:00" },
  { id: "26.1", type: "release", releaseTime: "2026-06-18T14:02:00+00:00" },
  { id: "25.4", type: "release", releaseTime: "2026-04-02T10:45:00+00:00" },
  { id: "1.21.8", type: "release", releaseTime: "2026-02-11T16:20:00+00:00" },
  { id: "1.20.1", type: "release", releaseTime: "2025-06-12T12:00:00+00:00" },
  { id: "1.7.10", type: "release", releaseTime: "2014-06-26T00:00:00+00:00" },
];

const LOADERS: LoaderOption[] = [
  { version: "0.20.1", stable: true },
  { version: "0.21.0-beta.2", stable: false },
];

const SETTINGS: SettingsView = {
  gameDir: "C:\\Users\\Luoqingci\\Desktop\\QingciCraftLauncher\\game",
  defaultGameDir: "C:\\Users\\Luoqingci\\Desktop\\QingciCraftLauncher\\game",
  isCustom: false,
  fallbackReason: null,
  playerName: "清辞",
  memoryMb: 6144,
  javaPath: null,
  memoryRange: [512, 65536],
  selectedVersion: null,
  loaderVersion: null,
  downloadSource: "official",
  instances: [{ name: "默认", gameDir: "C:\\games\\qcl", isDefaultDir: false, active: true, selectedVersion: null, loaderVersion: null }],
  activeInstance: "默认",
};

const LOGS: LogEntry[] = [
  { id: 1, stream: "system", text: "启动 fabric-loader-0.20.1-26.3（Java 25.0.2）" },
  { id: 2, stream: "stdout", text: "[12:04:31] [main/INFO]: Loading Minecraft 26.3 with Fabric Loader 0.20.1" },
  { id: 3, stream: "stdout", text: "[12:04:31] [main/INFO]: Loading 63 mods:" },
  { id: 4, stream: "stdout", text: "[12:04:31] [main/INFO]: - fabric-api 0.140.2+26.3" },
  { id: 5, stream: "stdout", text: "[12:04:31] [main/INFO]: - fabricloader 0.20.1" },
  { id: 6, stream: "stdout", text: "[12:04:32] [main/INFO]: SpongePowered MIXIN Config loaded." },
  { id: 7, stream: "stdout", text: "[12:04:33] [Render thread/INFO]: Backend library: LWJGL 3.4.1" },
  { id: 8, stream: "stderr", text: "[12:04:34] [Render thread/WARN]: Failed to fetch user properties" },
  { id: 9, stream: "stdout", text: "[12:04:35] [Render thread/INFO]: OpenAL initialized on device" },
  { id: 10, stream: "stdout", text: "[12:04:36] [Render thread/INFO]: Narrator library for x64 successfully loaded" },
  { id: 11, stream: "stdout", text: "[12:04:37] [Render thread/INFO]: Sound engine started" },
  { id: 12, stream: "stdout", text: "[12:04:38] [Render thread/INFO]: Created: 1024x512x4 minecraft:textures/atlas/blocks.png" },
];

const STATUS: GameStatus = { running: false, version: null };

function Preview() {
  const [selected, setSelected] = useState<string | null>("26.3");
  const [loader, setLoader] = useState("0.20.1");
  const [logs, setLogs] = useState(LOGS);

  return (
    <LauncherView
      versions={{ status: "ready", versions: VERSIONS }}
      selected={selected}
      loaders={LOADERS}
      loader={loader}
      loaderBusy={false}
      busy={false}
      progress={null}
      status={STATUS}
      logs={logs}
      // 想看崩溃卡片时，把这里换成 { exitCode: 1, headline: "…", lines: ["…"] }
      crash={null}
      onDismissCrash={() => {}}
      onDismissMessage={() => {}}
      onDismissError={() => {}}
      // 想看更新卡片时，把这里换成 { kind: "available", info: { version: "0.1.1", notes: "…" } }
      account={null}
      msaLogin
      loginPhase={{ kind: "idle" }}
      onLogin={() => {}}
      onCancelLogin={() => {}}
      onSignOut={() => {}}
      update={{ kind: "idle" }}
      portable={false}
      onCheckUpdate={() => {}}
      onInstallUpdate={() => {}}
      settings={SETTINGS}
      javaOptions={[]}
      savingSettings={false}
      settingsError={null}
      message="fabric-loader-0.20.1-26.3 已就绪：版本文件 3 个下载 / 76 个跳过，资产 5147 个，natives 6 个"
      error={null}
      onSelectVersion={setSelected}
      onLoaderChange={setLoader}
      onInstall={() => {}}
      onLaunch={() => {}}
      onStop={() => {}}
      onClearLogs={() => setLogs([])}
      onSaveSettings={() => {}}
      onRefreshJava={() => {}}
      onSwitchInstance={() => {}}
      onCreateInstance={() => {}}
      onRenameInstance={() => {}}
      onDeleteInstance={() => {}}
    />
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Preview />
  </StrictMode>,
);
