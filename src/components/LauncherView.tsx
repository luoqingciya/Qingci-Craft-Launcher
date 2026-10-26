// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { useState } from "react";

import type {
  GameStatus,
  InstallProgressEvent,
  JavaOption,
  LoaderOption,
  SettingsInput,
  SettingsView,
  VersionSummary,
} from "../api";
import type { LogEntry } from "../lib/logs";
import { InstallProgressBar, SettingsPanel } from "./SettingsPanel";
import { LoaderSelect } from "./LoaderSelect";
import { LogPanel } from "./LogPanel";
import { VersionList } from "./VersionList";

/** 版本清单的加载状态。 */
export type VersionsState =
  | { status: "loading" }
  | { status: "ready"; versions: VersionSummary[] }
  | { status: "failed"; message: string };

/**
 * 启动器主界面。
 *
 * 纯展示组件：所有状态与回调都由 `App` 传入。这样做有两个好处——
 * 界面可以用假数据单独渲染（视觉调整不必跑起 Tauri），也能对布局做快照式测试。
 */
export function LauncherView({
  versions,
  selected,
  loaders,
  loader,
  loaderBusy,
  busy,
  progress,
  status,
  logs,
  settings,
  javaOptions,
  savingSettings,
  settingsError,
  message,
  error,
  onSelectVersion,
  onLoaderChange,
  onInstall,
  onLaunch,
  onStop,
  onClearLogs,
  onSaveSettings,
  onRefreshJava,
}: {
  versions: VersionsState;
  selected: string | null;
  loaders: LoaderOption[];
  loader: string;
  loaderBusy: boolean;
  busy: boolean;
  progress: InstallProgressEvent | null;
  status: GameStatus;
  logs: LogEntry[];
  settings: SettingsView | null;
  javaOptions: JavaOption[];
  savingSettings: boolean;
  settingsError: string | null;
  message: string | null;
  error: string | null;
  onSelectVersion: (id: string) => void;
  onLoaderChange: (value: string) => void;
  onInstall: () => void;
  onLaunch: () => void;
  onStop: () => void;
  onClearLogs: () => void;
  onSaveSettings: (input: SettingsInput) => void;
  onRefreshJava: () => void;
}) {
  const [settingsOpen, setSettingsOpen] = useState(false);

  const ready = versions.status === "ready";
  const canAct = ready && selected !== null && !busy;
  const running = status.running;

  return (
    <div className="flex h-full flex-col bg-[var(--color-surface)]">
      <header className="flex h-12 shrink-0 items-center justify-between border-b border-[var(--color-border-subtle)] px-4">
        <div className="flex items-center gap-2.5">
          <span className="flex h-6 w-6 items-center justify-center rounded-md bg-gradient-to-br from-[var(--color-accent)] to-[#7c5cff] text-[13px] font-semibold text-white">
            清
          </span>
          <span className="text-[13px] font-semibold tracking-tight">清辞</span>
          <span className="text-[11px] text-[var(--color-text-muted)]">
            Qingci Craft Launcher
          </span>
        </div>

        <div className="flex items-center gap-2">
          <StatusChip running={running} version={status.version} />
          <button
            type="button"
            onClick={() => setSettingsOpen(true)}
            className="rounded-md px-2.5 py-1 text-[11px] text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-text-primary)]"
          >
            设置
          </button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <aside className="flex w-[300px] shrink-0 flex-col border-r border-[var(--color-border-subtle)]">
          <div className="flex h-9 shrink-0 items-center justify-between px-4">
            <h2 className="text-[11px] font-medium uppercase tracking-wider text-[var(--color-text-secondary)]">
              版本
            </h2>
            {ready && (
              <span className="text-[10px] tabular-nums text-[var(--color-text-muted)]">
                {versions.versions.length} 个
              </span>
            )}
          </div>

          {versions.status === "loading" && (
            <p className="px-4 py-6 text-xs text-[var(--color-text-secondary)]">
              正在获取版本清单…
            </p>
          )}
          {versions.status === "failed" && (
            <p className="px-4 py-6 text-xs text-[var(--color-danger)]">
              获取版本清单失败：{versions.message}
            </p>
          )}
          {versions.status === "ready" && (
            <VersionList
              versions={versions.versions}
              selected={selected}
              disabled={busy || running}
              onSelect={onSelectVersion}
            />
          )}
        </aside>

        <div className="flex min-w-0 flex-1 flex-col">
          <section className="shrink-0 px-4 pb-1 pt-4">
            <div className="rounded-xl bg-[var(--color-surface-raised)] p-4 ring-1 ring-inset ring-[var(--color-border-subtle)]">
              <div className="flex flex-wrap items-end justify-between gap-3">
                <div className="min-w-0">
                  <div className="text-[11px] uppercase tracking-wider text-[var(--color-text-muted)]">
                    当前版本
                  </div>
                  <div className="mt-1 flex items-center gap-2">
                    <span
                      className={`truncate font-mono text-lg ${
                        selected
                          ? "text-[var(--color-text-primary)]"
                          : "text-[var(--color-text-muted)]"
                      }`}
                    >
                      {selected ?? "未选择"}
                    </span>
                    {loader && (
                      <span className="rounded bg-[var(--color-accent-soft)] px-1.5 py-0.5 text-[10px] text-[var(--color-accent)]">
                        Fabric {loader}
                      </span>
                    )}
                  </div>
                </div>

                {selected && (
                  <LoaderSelect
                    loaders={loaders}
                    value={loader}
                    busy={loaderBusy}
                    disabled={!canAct || running}
                    onChange={onLoaderChange}
                  />
                )}
              </div>

              <div className="mt-4 flex items-stretch gap-2">
                <button
                  type="button"
                  onClick={onLaunch}
                  disabled={!canAct || running}
                  className="h-10 flex-1 rounded-lg bg-[var(--color-accent)] text-[13px] font-medium text-white shadow-[0_6px_16px_-6px_rgba(91,140,255,0.8)] transition-all hover:bg-[var(--color-accent-strong)] active:translate-y-px disabled:bg-[var(--color-surface-hover)] disabled:text-[var(--color-text-muted)] disabled:shadow-none"
                >
                  {running ? "游戏运行中" : "启动游戏"}
                </button>
                <button
                  type="button"
                  onClick={onInstall}
                  disabled={!canAct || running}
                  className="h-10 rounded-lg px-4 text-[12px] text-[var(--color-text-secondary)] ring-1 ring-inset ring-[var(--color-border-strong)] transition-colors hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-text-primary)] disabled:opacity-40"
                >
                  {busy ? "处理中…" : "安装 / 检查文件"}
                </button>
                <button
                  type="button"
                  onClick={onStop}
                  disabled={!running}
                  className="h-10 rounded-lg px-4 text-[12px] text-[var(--color-danger)] ring-1 ring-inset ring-[var(--color-danger)]/40 transition-colors hover:bg-[var(--color-danger)]/10 disabled:opacity-30"
                >
                  停止
                </button>
              </div>

              {progress && <InstallProgressBar progress={progress} />}

              {message && (
                <p className="mt-3 text-[11px] text-[var(--color-success)]">{message}</p>
              )}
              {error && <p className="mt-3 text-[11px] text-[var(--color-danger)]">{error}</p>}
            </div>
          </section>

          <LogPanel logs={logs} onClear={onClearLogs} />
        </div>
      </div>

      {settingsOpen && (
        <div className="absolute inset-0 z-50 flex justify-end">
          <button
            type="button"
            aria-label="关闭设置"
            onClick={() => setSettingsOpen(false)}
            className="absolute inset-0 bg-black/50 backdrop-blur-[1px]"
          />
          <div className="relative flex h-full w-[520px] max-w-full flex-col border-l border-[var(--color-border-subtle)] bg-[var(--color-surface)] shadow-2xl">
            <div className="flex h-12 shrink-0 items-center justify-between border-b border-[var(--color-border-subtle)] px-4">
              <h2 className="text-[13px] font-semibold">设置</h2>
              <button
                type="button"
                onClick={() => setSettingsOpen(false)}
                className="rounded-md px-2 py-1 text-[11px] text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-hover)]"
              >
                关闭
              </button>
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto p-4">
              {settings ? (
                <SettingsPanel
                  settings={settings}
                  javaOptions={javaOptions}
                  saving={savingSettings}
                  error={settingsError}
                  onSave={onSaveSettings}
                  onRefreshJava={onRefreshJava}
                  embedded
                />
              ) : (
                <p className="text-xs text-[var(--color-text-secondary)]">
                  {settingsError ? `读取设置失败：${settingsError}` : "正在读取设置…"}
                </p>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/** 运行状态。用圆点颜色传达状态，比纯文字更快被扫到。 */
function StatusChip({ running, version }: { running: boolean; version: string | null }) {
  return (
    <span
      role="status"
      className="flex items-center gap-1.5 rounded-full bg-[var(--color-surface-raised)] px-2.5 py-1 text-[11px] text-[var(--color-text-secondary)] ring-1 ring-inset ring-[var(--color-border-subtle)]"
    >
      <span
        className={`h-1.5 w-1.5 rounded-full ${
          running
            ? "bg-[var(--color-success)] shadow-[0_0_6px_var(--color-success)]"
            : "bg-[var(--color-text-muted)]"
        }`}
      />
      {running ? (version ?? "运行中") : "未运行"}
    </span>
  );
}
