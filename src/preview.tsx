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
import "./styles.css";

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
    />
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Preview />
  </StrictMode>,
);
