// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { useState } from "react";

// 用已生成好的 128px 图标而不是 929 KB 的源文件：界面里最大也只显示 56px。
import iconUrl from "../../src-tauri/icons/128x128.png";
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
import { applyTheme, loadTheme, type Theme } from "../lib/theme";
import { AboutPanel } from "./AboutPanel";
import { LoaderSelect } from "./LoaderSelect";
import { LogPanel } from "./LogPanel";
import { InstallProgressBar, SettingsPanel } from "./SettingsPanel";
import { VersionList } from "./VersionList";

/** 版本清单的加载状态。 */
export type VersionsState =
  | { status: "loading" }
  | { status: "ready"; versions: VersionSummary[] }
  | { status: "failed"; message: string };

/** 顶部分页。 */
type Tab = "home" | "settings" | "about";

const TABS: { id: Tab; label: string }[] = [
  { id: "home", label: "启动" },
  { id: "settings", label: "设置" },
  { id: "about", label: "关于" },
];

/**
 * 启动器主界面。
 *
 * 纯展示组件：业务状态与回调都由 `App` 传入，界面因此能用假数据单独渲染
 * （见 `src/preview.tsx`），也能直接对布局写测试。
 *
 * 自己持有的只有「当前标签页」与「主题」：前者是纯界面状态，
 * 后者要立刻生效，没必要绕一圈交给外层。
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
  const [tab, setTab] = useState<Tab>("home");
  const [theme, setTheme] = useState<Theme>(loadTheme);

  const ready = versions.status === "ready";
  const canAct = ready && selected !== null && !busy;
  const running = status.running;

  function toggleTheme() {
    const next: Theme = theme === "dark" ? "light" : "dark";
    setTheme(next);
    applyTheme(next);
  }

  return (
    <div className="flex h-full flex-col bg-[var(--color-surface)]">
      <header className="flex h-12 shrink-0 items-center gap-1 border-b border-[var(--color-border-subtle)] px-3">
        {/* 品牌区只留图标：窗口标题栏已经写着产品名，这里再重复一遍是浪费一行高度。 */}
        <img src={iconUrl} alt="清辞" className="mr-1 h-7 w-7 rounded-md" />

        <nav role="tablist" aria-label="主导航" className="flex items-center gap-0.5">
          {TABS.map((item) => {
            const active = tab === item.id;
            return (
              <button
                key={item.id}
                type="button"
                role="tab"
                aria-selected={active}
                onClick={() => setTab(item.id)}
                className={`relative rounded-md px-3 py-1.5 text-[12px] transition-colors ${
                  active
                    ? "text-[var(--color-text-primary)]"
                    : "text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-text-primary)]"
                }`}
              >
                {item.label}
                {active && (
                  <span className="absolute inset-x-2 -bottom-[9px] h-[2px] rounded-full bg-[var(--color-accent)]" />
                )}
              </button>
            );
          })}
        </nav>

        <div className="ml-auto flex items-center gap-2">
          <StatusChip running={running} version={status.version} />
          <button
            type="button"
            onClick={toggleTheme}
            aria-label={theme === "dark" ? "切换到浅色模式" : "切换到深色模式"}
            title={theme === "dark" ? "切换到浅色模式" : "切换到深色模式"}
            className="flex h-7 w-7 items-center justify-center rounded-md text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-text-primary)]"
          >
            {theme === "dark" ? <SunIcon /> : <MoonIcon />}
          </button>
        </div>
      </header>

      {tab === "home" && (
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
      )}

      {tab === "settings" && (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-[640px] px-6 py-6">
            {settings ? (
              <div className="rounded-xl bg-[var(--color-surface-raised)] p-5 ring-1 ring-inset ring-[var(--color-border-subtle)]">
                <SettingsPanel
                  settings={settings}
                  javaOptions={javaOptions}
                  saving={savingSettings}
                  error={settingsError}
                  onSave={onSaveSettings}
                  onRefreshJava={onRefreshJava}
                  embedded
                />
              </div>
            ) : (
              <p className="text-xs text-[var(--color-text-secondary)]">
                {settingsError ? `读取设置失败：${settingsError}` : "正在读取设置…"}
              </p>
            )}
          </div>
        </div>
      )}

      {tab === "about" && (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <AboutPanel />
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

function SunIcon() {
  return (
    <svg
      width="15"
      height="15"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
    >
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
    </svg>
  );
}

function MoonIcon() {
  return (
    <svg
      width="15"
      height="15"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z" />
    </svg>
  );
}
