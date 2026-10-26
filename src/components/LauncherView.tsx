// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { useState } from "react";

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
import { LaunchPanel } from "./LaunchPanel";
import { LogPanel } from "./LogPanel";
import { SettingsPanel } from "./SettingsPanel";
import { VersionList } from "./VersionList";

/** 版本清单的加载状态。 */
export type VersionsState =
  | { status: "loading" }
  | { status: "ready"; versions: VersionSummary[] }
  | { status: "failed"; message: string };

/** 顶部分页。 */
type Tab = "home" | "versions" | "settings" | "about";

const TABS: { id: Tab; label: string }[] = [
  { id: "home", label: "启动" },
  { id: "versions", label: "版本" },
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
  const playerName = settings?.playerName ?? "Player";

  function toggleTheme() {
    const next: Theme = theme === "dark" ? "light" : "dark";
    setTheme(next);
    applyTheme(next);
  }

  return (
    <div className="flex h-full flex-col bg-[var(--color-surface)]">
      {/* 顶栏用强调色渐变：整窗的主色块，避免界面看起来只有一片灰。 */}
      <header className="flex h-14 shrink-0 items-center gap-3 border-b border-[var(--color-border-subtle)] bg-[var(--color-surface-raised)] px-4">
        <img src={iconUrl} alt="" className="h-8 w-8 rounded-lg" />

        <nav role="tablist" aria-label="主导航" className="ml-4 flex items-center gap-1">
          {TABS.map((item) => {
            const active = tab === item.id;
            return (
              <button
                key={item.id}
                type="button"
                role="tab"
                aria-selected={active}
                onClick={() => setTab(item.id)}
                className={`rounded-lg px-3.5 py-1.5 text-[14px] transition-colors ${
                  active
                    ? "bg-[var(--color-accent-soft)] font-medium text-[var(--color-accent-text)]"
                    : "text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-text-primary)]"
                }`}
              >
                {item.label}
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
            className="flex h-8 w-8 items-center justify-center rounded-lg text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-text-primary)]"
          >
            {theme === "dark" ? <SunIcon /> : <MoonIcon />}
          </button>
        </div>
      </header>

      {tab === "home" && (
        <div className="flex min-h-0 flex-1">
          <div className="w-[360px] shrink-0 border-r border-[var(--color-border-subtle)] p-4">
            <LaunchPanel
              playerName={playerName}
              selected={selected}
              selectedVersion={
                ready ? versions.versions.find((item) => item.id === selected) : undefined
              }
              loaders={loaders}
              loader={loader}
              loaderBusy={loaderBusy}
              busy={busy}
              status={status}
              progress={progress}
              message={message}
              error={error}
              canAct={canAct}
              onLoaderChange={onLoaderChange}
              onLaunch={onLaunch}
              onInstall={onInstall}
              onStop={onStop}
              onBrowseVersions={() => setTab("versions")}
            />
          </div>

          <div className="flex min-w-0 flex-1 flex-col bg-gradient-to-br from-[var(--color-canvas-from)] to-[var(--color-canvas-to)] p-4">
            <LogPanel logs={logs} onClear={onClearLogs} />
          </div>
        </div>
      )}

      {tab === "versions" && (
        <div className="flex min-h-0 flex-1">
          <aside className="flex w-[380px] shrink-0 flex-col border-r border-[var(--color-border-subtle)]">
            <div className="flex h-11 shrink-0 items-center justify-between px-4">
              <h2 className="text-[12px] font-medium uppercase tracking-wider text-[var(--color-text-secondary)]">
                版本列表
              </h2>
              {ready && (
                <span className="text-[12px] tabular-nums text-[var(--color-text-muted)]">
                  {versions.versions.length} 个
                </span>
              )}
            </div>

            {versions.status === "loading" && (
              <p className="px-4 py-6 text-[13px] text-[var(--color-text-secondary)]">
                正在获取版本清单…
              </p>
            )}
            {versions.status === "failed" && (
              <p className="px-4 py-6 text-[13px] text-[var(--color-danger)]">
                获取版本清单失败：{versions.message}
              </p>
            )}
            {versions.status === "ready" && (
              <VersionList
                versions={versions.versions}
                selected={selected}
                disabled={busy || running}
                onSelect={(id) => {
                  onSelectVersion(id);
                  setTab("home");
                }}
              />
            )}
          </aside>

          <div className="min-w-0 flex-1 bg-gradient-to-br from-[var(--color-canvas-from)] to-[var(--color-canvas-to)] p-6">
            <VersionDetail
              version={ready ? versions.versions.find((item) => item.id === selected) : undefined}
              selected={selected}
              loader={loader}
            />
          </div>
        </div>
      )}

      {tab === "settings" && (
        <div className="min-h-0 flex-1 overflow-y-auto bg-gradient-to-br from-[var(--color-canvas-from)] to-[var(--color-canvas-to)]">
          <div className="mx-auto w-full max-w-[680px] px-6 py-6">
            {settings ? (
              <div className="rounded-2xl bg-[var(--color-surface-raised)] p-5 ring-1 ring-inset ring-[var(--color-border-subtle)]">
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
              <p className="text-[13px] text-[var(--color-text-secondary)]">
                {settingsError ? `读取设置失败：${settingsError}` : "正在读取设置…"}
              </p>
            )}
          </div>
        </div>
      )}

      {tab === "about" && (
        <div className="min-h-0 flex-1 overflow-y-auto bg-gradient-to-br from-[var(--color-canvas-from)] to-[var(--color-canvas-to)]">
          <AboutPanel />
        </div>
      )}
    </div>
  );
}

/** 选中版本的详情。让「版本」页右侧不至于空着，也给一个回到启动的入口。 */
function VersionDetail({
  version,
  selected,
  loader,
}: {
  version: VersionSummary | undefined;
  selected: string | null;
  loader: string;
}) {
  if (!selected) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-[13px] text-[var(--color-text-muted)]">从左侧选择一个版本</p>
      </div>
    );
  }

  return (
    <div className="mx-auto w-full max-w-[560px]">
      <div className="text-[12px] uppercase tracking-wider text-[var(--color-text-muted)]">
        已选版本
      </div>
      <div className="mt-1 font-mono text-[26px] leading-tight text-[var(--color-text-primary)]">
        {selected}
      </div>

      <div className="mt-4 flex flex-wrap gap-2 text-[12px]">
        {version && (
          <>
            <Tag>{version.type === "release" ? "正式版" : "快照"}</Tag>
            <Tag>{version.releaseTime.slice(0, 10)}</Tag>
          </>
        )}
        {loader && <Tag accent>Fabric {loader}</Tag>}
      </div>

      <p className="mt-6 text-[13px] leading-relaxed text-[var(--color-text-secondary)]">
        点击左侧任意版本即可切换，并自动回到「启动」页。
      </p>
    </div>
  );
}

function Tag({ children, accent = false }: { children: React.ReactNode; accent?: boolean }) {
  return (
    <span
      className={`rounded-lg px-2 py-1 ${
        accent
          ? "bg-[var(--color-accent-soft)] text-[var(--color-accent-text)]"
          : "bg-[var(--color-surface-raised)] text-[var(--color-text-secondary)] ring-1 ring-inset ring-[var(--color-border-subtle)]"
      }`}
    >
      {children}
    </span>
  );
}

/** 运行状态。顶栏是彩色的，所以这里用半透明白底而不是灰底。 */
function StatusChip({ running, version }: { running: boolean; version: string | null }) {
  return (
    <span
      role="status"
      className="flex items-center gap-1.5 rounded-full bg-[var(--color-surface)] px-3 py-1 text-[12px] text-[var(--color-text-secondary)] ring-1 ring-inset ring-[var(--color-border-subtle)]"
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
      width="16"
      height="16"
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
      width="16"
      height="16"
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
