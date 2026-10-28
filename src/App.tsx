// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
  createInstance,
  deleteInstance,
  listMods,
  listVersions,
  openGameDir as openDir,
  renameInstance,
  switchInstance,
  rememberSelection,
  runtimeInfo,
  type RuntimeInfo,
  saveSettings,
  stopGame,
  type CrashReport,
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
import { summarizeCrash } from "./lib/crash";
import { appendLog, type LogEntry, type LogStreamKind } from "./lib/logs";
import { useAccount } from "./lib/useAccount";
import { useUpdate } from "./lib/useUpdate";

export default function App() {
  const [state, setState] = useState<VersionsState>({ status: "loading" });
  const [settings, setSettings] = useState<SettingsView | null>(null);
  // 启动页要显示「已装模组 N 个」。读不出来时保持 null，界面显示「读取中…」。
  const [modCount, setModCount] = useState<number | null>(null);
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

  // 成功提示几秒后自动收起。
  //
  // 定时器放在状态的所有者这里，而不是展示组件里：展示组件用 effect 改自己的
  // state 会被 react-hooks 拦下（也确实容易写出竞态），而「一条提示活多久」
  // 本来就是业务决定，不是渲染细节。
  const messageTimer = useRef<number | null>(null);
  const showMessage = useCallback((text: string | null) => {
    if (messageTimer.current !== null) {
      window.clearTimeout(messageTimer.current);
      messageTimer.current = null;
    }
    setMessage(text);
    if (text !== null) {
      messageTimer.current = window.setTimeout(() => setMessage(null), 5000);
    }
  }, []);

  const [status, setStatus] = useState<GameStatus>({ running: false, version: null });
  const [logs, setLogs] = useState<LogEntry[]>([]);
  // 上次退出码：null 表示被用户终止，0 表示正常退出。
  const [exitCode, setExitCode] = useState<number | null | undefined>(undefined);
  // 这一局的崩溃报告（游戏自己写的）。与退出码同生命周期。
  const [crashReport, setCrashReport] = useState<CrashReport | null>(null);
  const [crashDismissed, setCrashDismissed] = useState(false);
  const logId = useRef(0);

  // 更新：挂载时静默查一次，结果同时驱动「关于」标签上的提示点。
  const update = useUpdate();
  const account = useAccount();
  // 便携版没有卸载器，装不了更新。默认按便携版处理——宁可少给一个按钮，
  // 也不要让用户点了「下载并安装」之后被装出一份新的副本。
  // 运行环境信息：既决定「能不能自更新」，也是关于页要显示的诊断信息。
  // 查不到时按最保守的情况处理（当作便携版、不给登录入口）。
  const [runtime, setRuntime] = useState<RuntimeInfo | null>(null);
  const portable = runtime ? !runtime.installed : true;
  const msaLogin = runtime?.msaLogin ?? false;

  useEffect(() => {
    void runtimeInfo()
      .then((info) => {
        setRuntime(info);
      })
      .catch(() => {
        // 查不到就维持保守判断：按便携版、不给登录入口处理。
      });
  }, []);

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
        const { code, durationSecs, crashReport: report } = event.payload;
        setExitCode(code);
        setCrashReport(report);
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

  const crash = useMemo(() => {
    if (exitCode === undefined || crashDismissed) {
      return null;
    }
    return summarizeCrash(logs, exitCode, crashReport);
  }, [logs, exitCode, crashReport, crashDismissed]);

  const saveSettingsInput = useCallback(async (input: SettingsInput) => {
    setSavingSettings(true);
    setSettingsError(null);
    try {
      setSettings(await saveSettings(input));
      showMessage("设置已保存");
    } catch (cause: unknown) {
      setSettingsError(String(cause));
    } finally {
      setSavingSettings(false);
    }
  }, [showMessage]);

  /**
   * 切换/新建/删除实例后的统一收尾。
   *
   * 不能只 setSettings：版本清单来自游戏目录，换了实例就是另一份清单；
   * 选择也要跟着换成该实例记下的那个，否则界面上会显示上一实例的版本。
   */
  const adoptInstance = useCallback(
    async (apply: () => Promise<SettingsView>) => {
      setSavingSettings(true);
      setSettingsError(null);
      try {
        const view = await apply();
        setSettings(view);

        setSelected(view.selectedVersion);
        setLoader(view.loaderVersion ?? "");
        if (view.selectedVersion) {
          void loadLoaders(view.selectedVersion);
        } else {
          setLoaders([]);
        }

        setState({ status: "ready", versions: await listVersions() });
      } catch (cause: unknown) {
        setSettingsError(String(cause));
      } finally {
        setSavingSettings(false);
      }
    },
    [loadLoaders]
  );

  const switchInstanceByName = useCallback(
    (name: string) => {
      void adoptInstance(() => switchInstance(name));
    },
    [adoptInstance]
  );

  const createInstanceByName = useCallback(
    (name: string) => {
      void adoptInstance(() => createInstance(name, null));
    },
    [adoptInstance]
  );

  const renameInstanceByNames = useCallback(
    (from: string, to: string) => {
      void adoptInstance(() => renameInstance(from, to));
    },
    [adoptInstance]
  );

  const deleteInstanceByName = useCallback(
    (name: string) => {
      void adoptInstance(() => deleteInstance(name));
    },
    [adoptInstance]
  );

  /**
   * 刷新「已装模组」数量。
   *
   * 失败时置 `null` 而不是 0：0 会被读成「一个都没装」，那是错误信息。
   */
  const refreshModCount = useCallback(async () => {
    try {
      setModCount((await listMods()).length);
    } catch {
      setModCount(null);
    }
  }, []);

  /**
   * 用系统文件管理器打开当前实例的游戏目录。
   *
   * 走自己的命令，而不是前端的 `plugin:opener|open_path`：后者需要单独放开一个
   * 作用域权限，而 `opener:default` 只含 open-url 与 reveal-item，点一下就是
   * 一句 ACL 拒绝。路径本来就在后端手里，也没必要让前端传。
   */
  const openGameDir = useCallback(async () => {
    try {
      await openDir();
    } catch (cause: unknown) {
      setError(`打开游戏目录失败：${String(cause)}`);
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
    showMessage(null);
    setProgress(null);
    try {
      const summary = await installGameVersion(selected, loader === "" ? null : loader);
      showMessage(
        `${summary.version} 已就绪：版本文件 ${summary.downloaded} 个下载 / ${summary.skipped} 个跳过，` +
          `资产 ${summary.assets} 个，natives ${summary.natives} 个`,
      );
    } catch (cause: unknown) {
      setError(String(cause));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }, [selected, loader, showMessage]);

  const launch = useCallback(async () => {
    if (!selected) {
      return;
    }
    // 目标版本由后端按「版本 + 加载器」推导，界面不再自己算——
    // 之前用「安装后记住的 id」，没点过安装就直接启动会静默启动原版。
    setBusy(true);
    setError(null);
    showMessage(null);
    setExitCode(undefined);
    setCrashReport(null);
    setCrashDismissed(false);
    // 启动前后端会补齐文件（幂等），有缺口时会发进度事件。
    // 先把上一次的进度清掉，结束时无论成败都清——否则那个「100%」的浮层会一直挂着。
    setProgress(null);
    try {
      await launchGame(selected, loader === "" ? null : loader);
      setStatus({ running: true, version: selected });
    } catch (cause: unknown) {
      setError(String(cause));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }, [selected, loader, showMessage]);

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
      crash={crash}
      onDismissCrash={() => setCrashDismissed(true)}
      onDismissMessage={() => showMessage(null)}
      onDismissError={() => setError(null)}
      update={update.state}
      portable={portable}
      runtime={runtime}
      account={account.account}
      msaLogin={msaLogin}
      loginPhase={account.phase}
      onLogin={account.start}
      onCancelLogin={account.cancel}
      onSignOut={account.signOut}
      onCheckUpdate={update.check}
      onInstallUpdate={update.install}
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
      onSwitchInstance={switchInstanceByName}
      modCount={modCount}
      onOpenGameDir={() => void openGameDir()}
      onRefreshModCount={() => void refreshModCount()}
      onCreateInstance={createInstanceByName}
      onRenameInstance={renameInstanceByNames}
      onDeleteInstance={deleteInstanceByName}
    />
  );
}
