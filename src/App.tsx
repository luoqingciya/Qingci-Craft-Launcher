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
  type LoaderKind,
  type LoaderOption,
  type SettingsInput,
  type SettingsView,
} from "./api";
import { LauncherView, type VersionsState } from "./components/LauncherView";
import { summarizeCrash } from "./lib/crash";
import { isLoaderVersionRejected, parseLoaderKind } from "./lib/loader";
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
  // 加载器种类。切种类要清掉版本号，否则会拿 Fabric 的版本号去装 Forge。
  const [loaderKind, setLoaderKind] = useState<LoaderKind>("fabric");
  const [loaderBusy, setLoaderBusy] = useState(false);
  // 加载器列表拉取失败的原因。空列表不能说明原因，界面上会被误读成「上游没有这个加载器」。
  const [loaderError, setLoaderError] = useState<string | null>(null);
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
  const loadLoaders = useCallback(async (id: string, kind: LoaderKind) => {
    const request = ++loaderRequest.current;
    setLoaderBusy(true);
    setLoaderError(null);
    // 先把上一份列表清掉：切种类后旧列表还留在屏幕上，用户点到的就是**别的加载器**
    // 的版本号（Forge 实例记下 Fabric 的 0.20.1，安装时报 404 就是这么来的）。
    setLoaders([]);

    try {
      const options = await listLoaders(id, kind);
      if (loaderRequest.current === request) {
        setLoaders(options);
      }
    } catch (cause: unknown) {
      // 拉不到加载器列表不影响原版安装，但**必须说出原因**：
      // 空列表会被读成「上游没有这个加载器」，而实际常常只是没连上。
      if (loaderRequest.current === request) {
        setLoaders([]);
        setLoaderError(String(cause));
      }
    } finally {
      if (loaderRequest.current === request) {
        setLoaderBusy(false);
      }
    }
  }, []);

  /** 重新拉一次当前版本 + 种类的加载器列表（选择器里的「重试」）。 */
  const retryLoaders = useCallback(() => {
    if (selected) {
      void loadLoaders(selected, loaderKind);
    }
  }, [selected, loaderKind, loadLoaders]);

  const selectVersion = useCallback(
    async (id: string) => {
      setSelected(id);
      setLoader("");
      // 换版本意味着加载器也要重选，所以这里连同加载器一起清掉。
      void rememberSelection(id, null);
      await loadLoaders(id, loaderKind);
    },
    [loadLoaders, loaderKind]
  );

  const changeLoader = useCallback(
    (value: string) => {
      setLoader(value);
      void rememberSelection(selected, value === "" ? null : value, loaderKind);
    },
    [selected, loaderKind]
  );

  /** 重新拉一次版本清单（版本选择器里的「重试」）。 */
  const reloadVersions = useCallback(async () => {
    setState({ status: "loading" });
    try {
      setState({ status: "ready", versions: await listVersions() });
    } catch (cause: unknown) {
      setState({ status: "failed", message: String(cause) });
    }
  }, []);

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
        // **种类也必须一起恢复**：否则切到 Forge 实例后界面仍以为自己是 Fabric，
        // 装的时候会拿 Forge 的版本号去请求 Fabric 的 profile（HTTP 400）。
        const kind = parseLoaderKind(view.loaderKind);
        setLoaderKind(kind);
        if (view.selectedVersion) {
          setSelected(view.selectedVersion);
          setLoader(view.loaderVersion ?? "");
          void loadLoaders(view.selectedVersion, kind);
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
   * 重新数一遍当前实例的模组。
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

  // 传下去的必须是稳定引用：内联箭头每次渲染都换身份，会让 LauncherView 里
  // 「回启动页就刷新」的 effect 每渲染一次都跑一遍（白白扫一次 mods 目录）。
  const refreshModCountNow = useCallback(() => {
    void refreshModCount();
  }, [refreshModCount]);

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

        // 与首次加载同样的同步：实例换了，版本、加载器、种类都要跟着换。
        const kind = parseLoaderKind(view.loaderKind);
        setLoaderKind(kind);
        setSelected(view.selectedVersion);
        setLoader(view.loaderVersion ?? "");
        // 上一个实例的退出码与崩溃摘要不属于新实例，留着会让人以为新实例崩过。
        setExitCode(null);
        setCrashReport(null);
        if (view.selectedVersion) {
          void loadLoaders(view.selectedVersion, kind);
        } else {
          setLoaders([]);
        }
        // `mods/` 每个实例各一份：不重数就会显示上一个实例的数字。
        void refreshModCount();

        // **不要 await 版本清单**：它是网络请求，官方源不通时能挂很久，
        // 而「保存中」的转圈就挂在这句上——用户看到的是「创建实例卡住」。
        // 实例本身已经建好了，清单晚一点到就行。
        setState({ status: "loading" });
        void listVersions()
          .then((versions) => setState({ status: "ready", versions }))
          .catch((cause: unknown) =>
            setState({ status: "failed", message: String(cause) })
          );
      } catch (cause: unknown) {
        setSettingsError(String(cause));
      } finally {
        setSavingSettings(false);
      }
    },
    [loadLoaders, refreshModCount]
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
    (name: string, deleteFiles: boolean) => {
      void adoptInstance(() => deleteInstance(name, deleteFiles));
    },
    [adoptInstance]
  );

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
      const summary = await installGameVersion(selected, loader === "" ? null : loader, loaderKind);
      showMessage(
        `${summary.version} 已就绪：版本文件 ${summary.downloaded} 个下载 / ${summary.skipped} 个跳过，` +
          `资产 ${summary.assets} 个，natives ${summary.natives} 个`,
      );
    } catch (cause: unknown) {
      const text = String(cause);
      setError(text);
      // 上游 4xx 拒绝了这个加载器版本号 = 它不属于当前种类（切种类时点到了旧列表）。
      // 清掉选择让「加载器」那行解锁，用户能重选；网络错误不动，重试即可。
      if (selected && isLoaderVersionRejected(text)) {
        setLoader("");
        void rememberSelection(selected, null, loaderKind);
      }
    } finally {
      setBusy(false);
      setProgress(null);
    }
  }, [selected, loader, loaderKind, showMessage]);

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
      loaderKind={loaderKind}
      onLoaderKindChange={(kind) => {
        setLoaderKind(kind);
        setLoader("");
        if (selected) {
          void loadLoaders(selected, kind);
          // 种类要立刻落盘：只改界面状态的话，切走实例或重启就丢了，
          // 下次安装又会拿 Forge 的版本号去问 Fabric。
          void rememberSelection(selected, null, kind);
        }
      }}
      loader={loader}
      loaderBusy={loaderBusy}
      loaderError={loaderError}
      onRetryLoaders={retryLoaders}
      onRetryVersions={() => void reloadVersions()}
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
      restoring={account.restoring}
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
      onRefreshModCount={refreshModCountNow}
      onCreateInstance={createInstanceByName}
      onRenameInstance={renameInstanceByNames}
      onDeleteInstance={deleteInstanceByName}
    />
  );
}
