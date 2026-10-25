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
  listJava,
  listVersions,
  saveSettings,
  stopGame,
  type GameExitEvent,
  type GameLogEvent,
  type GameStatus,
  type InstallProgressEvent,
  type JavaOption,
  type SettingsInput,
  type SettingsView,
  type VersionSummary,
} from "./api";
import { InstallProgressBar, SettingsPanel } from "./components/SettingsPanel";
import { LogPanel } from "./components/LogPanel";
import { appendLog, type LogEntry, type LogStreamKind } from "./lib/logs";
import { VersionList } from "./components/VersionList";

/** 版本清单的加载状态。用联合类型避免「既在加载又有错误」的非法状态。 */
type LoadState =
  | { status: "loading" }
  | { status: "ready"; versions: VersionSummary[] }
  | { status: "failed"; message: string };

export default function App() {
  const [state, setState] = useState<LoadState>({ status: "loading" });
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [javaOptions, setJavaOptions] = useState<JavaOption[]>([]);
  const [savingSettings, setSavingSettings] = useState(false);
  const [settingsError, setSettingsError] = useState<string | null>(null);

  const [selected, setSelected] = useState<string | null>(null);
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
        if (!cancelled) {
          setSettings(view);
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
  }, []);

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

  const install = useCallback(async () => {
    if (!selected) {
      return;
    }
    setBusy(true);
    setError(null);
    setMessage(null);
    setProgress(null);
    try {
      const summary = await installGameVersion(selected);
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
  }, [selected]);

  const launch = useCallback(async () => {
    if (!selected) {
      return;
    }
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      await launchGame(selected);
      setStatus({ running: true, version: selected });
    } catch (cause: unknown) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }, [selected]);

  const stop = useCallback(async () => {
    setError(null);
    try {
      await stopGame();
    } catch (cause: unknown) {
      setError(String(cause));
    }
  }, []);

  const canAct = state.status === "ready" && selected !== null && !busy;
  const runningLabel = status.running ? `运行中：${status.version ?? ""}` : "未运行";

  return (
    <main className="flex h-full flex-col gap-3 p-6">
      <header className="flex items-baseline justify-between">
        <h1 className="text-lg font-semibold tracking-tight">Qingci Craft Launcher</h1>
        <span className="text-xs text-[var(--color-text-secondary)]">{runningLabel}</span>
      </header>

      {settings ? (
        <SettingsPanel
          settings={settings}
          javaOptions={javaOptions}
          saving={savingSettings}
          error={settingsError}
          onSave={(input) => void saveSettingsInput(input)}
          onRefreshJava={() => void refreshJava()}
        />
      ) : (
        <section className="rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-surface-raised)] px-4 py-3 text-xs text-[var(--color-text-secondary)]">
          {settingsError ? `读取设置失败：${settingsError}` : "正在读取设置…"}
        </section>
      )}

      <section className="rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-surface-raised)] px-4 py-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="text-sm">
            {selected ? (
              <>
                已选版本 <span className="font-mono text-[var(--color-accent)]">{selected}</span>
              </>
            ) : (
              <span className="text-[var(--color-text-secondary)]">
                在下方列表中选择一个版本
              </span>
            )}
          </div>

          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => void install()}
              disabled={!canAct || status.running}
              className="rounded border border-[var(--color-border-subtle)] px-3 py-1.5 text-xs hover:bg-[var(--color-accent-soft)] disabled:opacity-40"
            >
              {busy ? "处理中…" : "安装 / 检查文件"}
            </button>
            <button
              type="button"
              onClick={() => void launch()}
              disabled={!canAct || status.running}
              className="rounded bg-[var(--color-accent)] px-3 py-1.5 text-xs font-medium text-white hover:opacity-90 disabled:opacity-40"
            >
              启动游戏
            </button>
            <button
              type="button"
              onClick={() => void stop()}
              disabled={!status.running}
              className="rounded border border-red-500/50 px-3 py-1.5 text-xs text-red-400 hover:bg-red-500/10 disabled:opacity-40"
            >
              停止
            </button>
          </div>
        </div>

        {progress && <InstallProgressBar progress={progress} />}

        {message && <p className="mt-2 text-xs text-emerald-400">{message}</p>}
        {error && <p className="mt-2 text-xs text-red-400">{error}</p>}
      </section>

      <div className="grid min-h-0 flex-1 grid-cols-5 gap-3">
        <div className="col-span-2 flex min-h-0 flex-col">
          {state.status === "loading" && (
            <p className="text-sm text-[var(--color-text-secondary)]">正在获取版本清单…</p>
          )}
          {state.status === "failed" && (
            <p className="text-sm text-red-400">获取版本清单失败：{state.message}</p>
          )}
          {state.status === "ready" && (
            <VersionList
              versions={state.versions}
              selected={selected}
              disabled={busy || status.running}
              onSelect={setSelected}
            />
          )}
        </div>

        <div className="col-span-3 flex min-h-0 flex-col">
          <LogPanel logs={logs} onClear={() => setLogs([])} />
        </div>
      </div>

      <footer className="text-xs text-[var(--color-text-secondary)]">
        账号体系与模组管理尚未接入，当前使用离线账户。
      </footer>
    </main>
  );
}
