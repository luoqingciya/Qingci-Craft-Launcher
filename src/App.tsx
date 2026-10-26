// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { useCallback, useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import {
  GAME_EXIT_EVENT,
  GAME_LOG_EVENT,
  INSTALL_PROGRESS_EVENT,
  gameStatus,
  getSettings,
  installGameVersion,
  launchGame,
  listLoaders,
  listJava,
  listVersions,
  rememberSelection,
  saveSettings,
  stopGame,
  type GameExitEvent,
  type GameLogEvent,
  type GameStatus,
  type InstallProgressEvent,
  type JavaOption,
  type LoaderOption,
  type SettingsInput,
  type SettingsView,
} from "./api";
import { LauncherView, type VersionsState } from "./components/LauncherView";
import { appendLog, type LogEntry, type LogStreamKind } from "./lib/logs";

export default function App() {
  const [state, setState] = useState<VersionsState>({ status: "loading" });
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [javaOptions, setJavaOptions] = useState<JavaOption[]>([]);
  const [savingSettings, setSavingSettings] = useState(false);
  const [settingsError, setSettingsError] = useState<string | null>(null);

  const [selected, setSelected] = useState<string | null>(null);
  // 加载器列表的请求序号：快速切换版本时丢弃过期响应，避免列表与选中版本错位。
  const loaderRequest = useRef(0);
  const [loaders, setLoaders] = useState<LoaderOption[]>([]);
  const [loader, setLoader] = useState("");
  const [loaderBusy, setLoaderBusy] = useState(false);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<InstallProgressEvent | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [status, setStatus] = useState<GameStatus>({ running: false, version: null });
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const logId = useRef(0);

  const pushLog = useCallback((entry: { stream: LogStreamKind; text: string }) => {
    logId.current += 1;
    const line: LogEntry = { ...entry, id: logId.current };
    setLogs((previous) => appendLog(previous, line));
  }, []);

  // 否则换了版本后还会用上一个版本的加载器 id 去启动。
  //
  // 刻意不用 useEffect：重置状态与发起请求都是由「用户点了某个版本」这一个事件引起的，
  // 放进 effect 会多一轮渲染，而且 effect 里同步 setState 本身就是反模式（lint 也会拦）。
  const loadLoaders = useCallback(async (id: string) => {
    const request = ++loaderRequest.current;
    setLoaderBusy(true);

    try {
      const options = await listLoaders(id);
      if (loaderRequest.current === request) {
        setLoaders(options);
      }
    } catch {
      // 拉不到加载器列表不影响原版安装，界面退化成只有「无（原版）」。
      if (loaderRequest.current === request) {
        setLoaders([]);
      }
    } finally {
      if (loaderRequest.current === request) {
        setLoaderBusy(false);
      }
    }
  }, []);

  const selectVersion = useCallback(
    async (id: string) => {
      setSelected(id);
      setLoader("");
      // 换版本意味着加载器也要重选，所以这里连同加载器一起清掉。
      void rememberSelection(id, null);
      await loadLoaders(id);
    },
    [loadLoaders]
  );

  const changeLoader = useCallback(
    (value: string) => {
      setLoader(value);
      void rememberSelection(selected, value === "" ? null : value);
    },
    [selected]
  );

  // 初始数据：版本清单、设置、运行状态。
  useEffect(() => {
    let cancelled = false;

    listVersions()
      .then((versions) => {
        if (!cancelled) {
          setState({ status: "ready", versions });
        }
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setState({ status: "failed", message: String(cause) });
        }
      });

    getSettings()
      .then((view) => {
        if (cancelled) {
          return;
        }
        setSettings(view);

        // 恢复上次选择。
        //
        // 刻意不走 selectVersion：它会把选择持久化成「版本 + 无加载器」，
        // 等于把刚读出来的加载器又抹掉。这里只设状态并拉一次加载器列表。
        if (view.selectedVersion) {
          setSelected(view.selectedVersion);
          setLoader(view.loaderVersion ?? "");
          void loadLoaders(view.selectedVersion);
        }
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setSettingsError(String(cause));
        }
      });

    listJava()
      .then((options) => {
        if (!cancelled) {
          setJavaOptions(options);
        }
      })
      .catch(() => {
        // 探测失败不影响设置页其它项，用户仍可手动填路径。
      });

    gameStatus()
      .then((current) => {
        if (!cancelled) {
          setStatus(current);
        }
      })
      .catch(() => {
        // 状态查询失败不影响使用，后续事件会纠正。
      });

    return () => {
      cancelled = true;
    };
    // loadLoaders 用 useCallback([]) 声明，引用稳定，不会让 effect 重跑。
  }, [loadLoaders]);

  // 事件订阅。监听是异步注册的，因此要先判断是否已卸载再决定保留还是立刻注销。
  useEffect(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];

    const register = (promise: Promise<UnlistenFn>) => {
      void promise.then((unlisten) => {
        if (disposed) {
          unlisten();
        } else {
          unlisteners.push(unlisten);
        }
      });
    };

    register(
      listen<InstallProgressEvent>(INSTALL_PROGRESS_EVENT, (event) => {
        setProgress(event.payload);
      }),
    );

    register(
      listen<GameLogEvent>(GAME_LOG_EVENT, (event) => {
        pushLog(event.payload);
      }),
    );

    register(
      listen<GameExitEvent>(GAME_EXIT_EVENT, (event) => {
        setStatus({ running: false, version: null });
        const { code, durationSecs } = event.payload;
        pushLog({
          stream: "system",
          text: `—— 游戏退出（${code ?? "被终止"}），运行 ${durationSecs.toFixed(1)} 秒 ——`,
        });
      }),
    );

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => {
        unlisten();
      });
    };
  }, [pushLog]);

  const saveSettingsInput = useCallback(async (input: SettingsInput) => {
    setSavingSettings(true);
    setSettingsError(null);
    try {
      setSettings(await saveSettings(input));
      setMessage("设置已保存");
    } catch (cause: unknown) {
      setSettingsError(String(cause));
    } finally {
      setSavingSettings(false);
    }
  }, []);

  const refreshJava = useCallback(async () => {
    try {
      setJavaOptions(await listJava());
    } catch (cause: unknown) {
      setSettingsError(String(cause));
    }
  }, []);

  // 选择版本时重新拉取加载器列表，并清掉「可启动版本」——
  const install = useCallback(async () => {
    if (!selected) {
      return;
    }
    setBusy(true);
    setError(null);
    setMessage(null);
    setProgress(null);
    try {
      const summary = await installGameVersion(selected, loader === "" ? null : loader);
      setMessage(
        `${summary.version} 已就绪：版本文件 ${summary.downloaded} 个下载 / ${summary.skipped} 个跳过，` +
          `资产 ${summary.assets} 个，natives ${summary.natives} 个`,
      );
    } catch (cause: unknown) {
      setError(String(cause));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }, [selected, loader]);

  const launch = useCallback(async () => {
    if (!selected) {
      return;
    }
    // 目标版本由后端按「版本 + 加载器」推导，界面不再自己算——
    // 之前用「安装后记住的 id」，没点过安装就直接启动会静默启动原版。
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      await launchGame(selected, loader === "" ? null : loader);
      setStatus({ running: true, version: selected });
    } catch (cause: unknown) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }, [selected, loader]);

  const stop = useCallback(async () => {
    setError(null);
    try {
      await stopGame();
    } catch (cause: unknown) {
      setError(String(cause));
    }
  }, []);

  return (
    <LauncherView
      versions={state}
      selected={selected}
      loaders={loaders}
      loader={loader}
      loaderBusy={loaderBusy}
      busy={busy}
      progress={progress}
      status={status}
      logs={logs}
      settings={settings}
      javaOptions={javaOptions}
      savingSettings={savingSettings}
      settingsError={settingsError}
      message={message}
      error={error}
      onSelectVersion={(id) => void selectVersion(id)}
      onLoaderChange={changeLoader}
      onInstall={() => void install()}
      onLaunch={() => void launch()}
      onStop={() => void stop()}
      onClearLogs={() => setLogs([])}
      onSaveSettings={(input) => void saveSettingsInput(input)}
      onRefreshJava={() => void refreshJava()}
    />
  );
}
