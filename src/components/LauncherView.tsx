// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { useEffect, useMemo, useState } from "react";

import iconUrl from "../../src-tauri/icons/128x128.png";
import type {
  GameStatus,
  InstallProgressEvent,
  JavaOption,
  LoaderOption,
  RuntimeInfo,
  SettingsInput,
  SettingsView,
  VersionSummary,
} from "../api";
import type { Account } from "../api";
import type { CrashSummary } from "../lib/crash";
import type { LogEntry } from "../lib/logs";
import { applyTheme, loadTheme, type Theme } from "../lib/theme";
import type { LoginPhase } from "../lib/useAccount";
import type { UpdateState } from "../lib/useUpdate";
import { AboutPanel } from "./AboutPanel";
import { CrashCard } from "./CrashCard";
import { LaunchPanel } from "./LaunchPanel";
import { LoginDialog } from "./LoginDialog";
import { LogPanel } from "./LogPanel";
import { StatusStack } from "./StatusStack";
import { ModsTab } from "./ModsTab";
import { PickerDialog, type PickerItem } from "./PickerDialog";
import { SettingsPanel } from "./SettingsPanel";

/** 版本清单的加载状态。 */
export type VersionsState =
  | { status: "loading" }
  | { status: "ready"; versions: VersionSummary[] }
  | { status: "failed"; message: string };

/** 顶部分页。 */
type Tab = "home" | "mods" | "settings" | "about";

const TABS: { id: Tab; label: string }[] = [
  { id: "home", label: "启动" },
  { id: "mods", label: "模组" },
  { id: "settings", label: "设置" },
  { id: "about", label: "关于" },
];

/**
 * 启动器主界面。
 *
 * 纯展示组件：业务状态与回调都由 `App` 传入，界面因此能用假数据单独渲染
 * （见 `src/preview.tsx`），也能直接对布局写测试。
 *
 * 自己持有的只有「当前标签页」「主题」和「哪个选择器开着」——都是纯界面状态。
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
  crash,
  account,
  restoring,
  msaLogin,
  loginPhase,
  update,
  portable,
  runtime,
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
  onDismissCrash,
  onDismissMessage,
  onDismissError,
  onCheckUpdate,
  onInstallUpdate,
  onLogin,
  onCancelLogin,
  onSignOut,
  onSaveSettings,
  onRefreshJava,
  onSwitchInstance,
  modCount,
  onOpenGameDir,
  onRefreshModCount,
  onCreateInstance,
  onRenameInstance,
  onDeleteInstance,
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
  /** 崩溃摘要；`null` 表示这次不是异常退出，或用户已收起。 */
  crash: CrashSummary | null;
  /** 已登录的账号，未登录为 `null`。 */
  account: Account | null;
  /** 是否仍在恢复上次的登录。 */
  restoring: boolean;
  /** 本次构建是否内置了微软登录能力。 */
  msaLogin: boolean;
  /** 登录对话框阶段。 */
  loginPhase: LoginPhase;
  /** 更新流程状态。 */
  update: UpdateState;
  /** 便携版无法自更新。 */
  portable: boolean;
  /** 运行环境信息，关于页显示诊断用；查不到时为 null。 */
  runtime: RuntimeInfo | null;
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
  onDismissCrash: () => void;
  /** 关掉浮层里的成功提示。 */
  onDismissMessage: () => void;
  /** 关掉浮层里的错误提示。 */
  onDismissError: () => void;
  onCheckUpdate: () => void;
  onInstallUpdate: () => void;
  onLogin: () => void;
  onCancelLogin: () => void;
  onSignOut: () => void;
  onSaveSettings: (input: SettingsInput) => void;
  onRefreshJava: () => void;
  onSwitchInstance: (name: string) => void;
  /** 已装模组数量，显示在启动页信息区。 */
  modCount: number | null;
  /** 用系统文件管理器打开当前实例的游戏目录。 */
  onOpenGameDir: () => void;
  /** 回到启动页时刷新模组数量（在模组页装完就该变）。 */
  onRefreshModCount: () => void;
  onCreateInstance: (name: string) => void;
  onRenameInstance: (from: string, to: string) => void;
  onDeleteInstance: (name: string) => void;
}) {
  const [tab, setTab] = useState<Tab>("home");

  // 回到启动页时刷新模组数量：在模组页装完/删完，这里应该立刻反映出来。
  useEffect(() => {
    if (tab === "home") {
      onRefreshModCount();
    }
  }, [tab, onRefreshModCount]);
  const [theme, setTheme] = useState<Theme>(loadTheme);
  const [picker, setPicker] = useState<"version" | "loader" | null>(null);

  const ready = versions.status === "ready";
  const canAct = ready && selected !== null && !busy;
  const running = status.running;
  const playerName = settings?.playerName ?? "Player";

  const selectedVersion = useMemo(
    () => (ready ? versions.versions.find((item) => item.id === selected) : undefined),
    [ready, versions, selected]
  );

  // 版本项：九百多条，交给选择器去搜索与截断。
  const versionItems: PickerItem[] = useMemo(() => {
    if (!ready) {
      return [];
    }
    return versions.versions.map((version) => ({
      id: version.id,
      label: version.id,
      meta: version.releaseTime.slice(0, 10),
      badge: version.type === "release" ? undefined : "快照",
    }));
  }, [ready, versions]);

  const loaderItems: PickerItem[] = useMemo(
    () => [
      { id: "", label: "无（原版）", meta: "不加载任何加载器", accent: true },
      ...loaders.map((option) => ({
        id: option.version,
        label: `Fabric ${option.version}`,
        meta: option.stable ? "稳定版" : "测试版",
      })),
    ],
    [loaders]
  );

  const loaderLabel = loader ? `Fabric ${loader}` : "无（原版）";

  function toggleTheme() {
    const next: Theme = theme === "dark" ? "light" : "dark";
    setTheme(next);
    applyTheme(next);
  }

  return (
    <div className="relative flex h-full flex-col bg-[var(--color-surface)]">
      {/* 顶栏保持中性：强调色只给按钮与选中态，整条铺色会显得廉价。 */}
      <header className="flex h-14 shrink-0 items-center gap-3 border-b border-[var(--color-border-subtle)] bg-[var(--color-surface-raised)] px-4">
        <img src={iconUrl} alt="" className="h-8 w-8 rounded-lg" />

        <nav role="tablist" aria-label="主导航" className="ml-2 flex items-center gap-1">
          {TABS.map((item) => {
            const active = tab === item.id;
            // 查到新版本时在「关于」上点一个提示点：自动检查不能是无声的，
            // 否则用户永远不知道有新版本。
            const notice = item.id === "about" && update.kind === "available";
            return (
              <button
                key={item.id}
                type="button"
                role="tab"
                aria-selected={active}
                onClick={() => setTab(item.id)}
                className={`relative rounded-lg px-3.5 py-1.5 text-[14px] transition-colors ${
                  active
                    ? "bg-[var(--color-accent-soft)] font-medium text-[var(--color-accent-text)]"
                    : "text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-text-primary)]"
                }`}
              >
                {item.label}
                {notice && (
                  <span
                    aria-label="有新版本"
                    title="有新版本"
                    className="absolute right-1 top-1 h-1.5 w-1.5 rounded-full bg-[var(--color-danger)]"
                  />
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
              account={account}
              restoring={restoring}
              msaLogin={msaLogin}
              instances={settings?.instances ?? []}
              activeInstance={settings?.activeInstance ?? ""}
              onSwitchInstance={onSwitchInstance}
              gameDir={settings?.gameDir ?? ""}
              modCount={modCount}
              onOpenGameDir={onOpenGameDir}
              selected={selected}
              selectedVersion={selectedVersion}
              loaderLabel={loaderLabel}
              busy={busy}
              status={status}
              canAct={canAct}
              onPickVersion={() => setPicker("version")}
              onPickLoader={() => setPicker("loader")}
              onLaunch={onLaunch}
              onInstall={onInstall}
              onStop={onStop}
              onLogin={onLogin}
              onSignOut={onSignOut}
            />
          </div>

          <div className="flex min-w-0 flex-1 flex-col bg-gradient-to-br from-[var(--color-canvas-from)] to-[var(--color-canvas-to)] p-4">
            {crash && <CrashCard summary={crash} onDismiss={onDismissCrash} />}
            <LogPanel logs={logs} onClear={onClearLogs} />
          </div>
        </div>
      )}

      {tab === "mods" && (
        <div className="min-h-0 flex-1 overflow-y-auto bg-gradient-to-br from-[var(--color-canvas-from)] to-[var(--color-canvas-to)]">
          <ModsTab version={selected} loader={loader} />
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
                  onSwitchInstance={onSwitchInstance}
                  onCreateInstance={onCreateInstance}
                  onRenameInstance={onRenameInstance}
                  onDeleteInstance={onDeleteInstance}
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
          <AboutPanel
            runtime={runtime}
            update={update}
            portable={portable}
            onCheck={onCheckUpdate}
            onInstall={onInstallUpdate}
          />
        </div>
      )}

      {picker === "version" && (
        <PickerDialog
          title="选择版本"
          placeholder="搜索版本号，如 1.20 或 26.3"
          items={versionItems}
          selectedId={selected}
          emptyHint={ready ? "没有匹配的版本" : "版本清单还没加载完"}
          onPick={(id) => {
            onSelectVersion(id);
            setPicker(null);
          }}
          onClose={() => setPicker(null)}
        />
      )}

      {picker === "loader" && (
        <PickerDialog
          title={loaderBusy ? "加载器（正在读取…）" : "选择加载器"}
          placeholder="搜索 Fabric 版本"
          items={loaderItems}
          selectedId={loader}
          emptyHint="这个版本没有可用的 Fabric 加载器"
          onPick={(value) => {
            onLoaderChange(value);
            setPicker(null);
          }}
          onClose={() => setPicker(null)}
        />
      )}

      <LoginDialog phase={loginPhase} onCancel={onCancelLogin} />

      {/* 下载进度与提示浮在页面上：进出都不改变任何组件的位置。 */}
      <StatusStack
        progress={progress}
        message={message}
        error={error}
        mirrored={settings?.downloadSource === "bmclapi"}
        onDismissMessage={onDismissMessage}
        onDismissError={onDismissError}
      />
    </div>
  );
}

/** 运行状态。 */
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
