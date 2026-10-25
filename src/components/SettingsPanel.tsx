import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import type { InstallProgressEvent, JavaOption, SettingsInput, SettingsView } from "../api";
import { phaseLabel } from "../lib/format";

/** 表单草稿。内存用字符串保存，才能表示「留空 = 官方默认」。 */
interface Draft {
  gameDir: string;
  playerName: string;
  memory: string;
  javaPath: string;
}

function toDraft(settings: SettingsView): Draft {
  return {
    gameDir: settings.gameDir,
    playerName: settings.playerName,
    memory: settings.memoryMb === null ? "" : String(settings.memoryMb),
    javaPath: settings.javaPath ?? "",
  };
}

/**
 * 设置面板。
 *
 * 采用「编辑草稿 → 保存」而不是每次改动立刻落盘：内存与 Java 这类字段
 * 边打字边保存会产生一堆中间态（输入 "4" 的瞬间就被当成 4 MB 写进配置）。
 */
export function SettingsPanel({
  settings,
  javaOptions,
  saving,
  error,
  onSave,
  onRefreshJava,
}: {
  settings: SettingsView;
  javaOptions: JavaOption[];
  saving: boolean;
  error: string | null;
  onSave: (input: SettingsInput) => void;
  onRefreshJava: () => void;
}) {
  const [draft, setDraft] = useState<Draft>(() => toDraft(settings));
  const [baseline, setBaseline] = useState<SettingsView>(settings);
  const [localError, setLocalError] = useState<string | null>(null);

  // 外部设置变化时同步草稿（例如保存成功后后端返回了规范化后的路径）。
  //
  // 用「渲染期调整状态」而不是 useEffect：effect 里 setState 会多跑一轮渲染，
  // 而且会让界面先闪一帧旧值。React 文档对「props 变化要重置 state」推荐的正是这种写法。
  if (baseline !== settings) {
    setBaseline(settings);
    setDraft(toDraft(settings));
  }

  const dirty =
    draft.gameDir !== settings.gameDir ||
    draft.playerName !== settings.playerName ||
    draft.memory !== (settings.memoryMb === null ? "" : String(settings.memoryMb)) ||
    draft.javaPath !== (settings.javaPath ?? "");

  const [minMemory, maxMemory] = settings.memoryRange;

  function submit() {
    setLocalError(null);

    const trimmedMemory = draft.memory.trim();
    let memoryMb: number | null = null;
    if (trimmedMemory !== "") {
      const parsed = Number(trimmedMemory);
      if (!Number.isInteger(parsed)) {
        setLocalError("内存必须是整数（MB）");
        return;
      }
      if (parsed < minMemory || parsed > maxMemory) {
        setLocalError(`内存必须在 ${minMemory}–${maxMemory} MB 之间`);
        return;
      }
      memoryMb = parsed;
    }

    if (draft.playerName.trim() === "") {
      setLocalError("玩家名不能为空");
      return;
    }

    onSave({
      gameDir: draft.gameDir.trim() === "" ? null : draft.gameDir.trim(),
      playerName: draft.playerName.trim(),
      memoryMb,
      javaPath: draft.javaPath.trim() === "" ? null : draft.javaPath.trim(),
    });
  }

  async function pickDirectory() {
    const picked = await open({ directory: true, multiple: false, title: "选择游戏目录" });
    if (typeof picked === "string") {
      setDraft((current) => ({ ...current, gameDir: picked }));
    }
  }

  async function pickJava() {
    const picked = await open({
      multiple: false,
      title: "选择 java 可执行文件",
      filters: [{ name: "Java", extensions: ["exe"] }],
    });
    if (typeof picked === "string") {
      setDraft((current) => ({ ...current, javaPath: picked }));
    }
  }

  const message = localError ?? error;

  return (
    <section className="rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-surface-raised)] px-4 py-3">
      <div className="mb-3 flex items-center justify-between">
        <h2 className="text-sm font-medium">设置</h2>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => setDraft(toDraft(settings))}
            disabled={!dirty || saving}
            className="rounded border border-[var(--color-border-subtle)] px-3 py-1.5 text-xs hover:bg-[var(--color-accent-soft)] disabled:opacity-40"
          >
            撤销
          </button>
          <button
            type="button"
            onClick={submit}
            disabled={!dirty || saving}
            className="rounded bg-[var(--color-accent)] px-3 py-1.5 text-xs font-medium text-white hover:opacity-90 disabled:opacity-40"
          >
            {saving ? "保存中…" : "保存"}
          </button>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-3 text-xs lg:grid-cols-2">
        {/* 游戏目录 */}
        <div className="lg:col-span-2">
          <label className="flex items-center gap-2 text-[var(--color-text-secondary)]">
            游戏目录
            {settings.isCustom && (
              <span className="rounded bg-[var(--color-accent-soft)] px-1.5 py-0.5 text-[10px] text-[var(--color-accent)]">
                自定义
              </span>
            )}
          </label>
          <div className="mt-1 flex gap-2">
            <input
              value={draft.gameDir}
              onChange={(event) => setDraft((c) => ({ ...c, gameDir: event.target.value }))}
              spellCheck={false}
              className="min-w-0 flex-1 rounded border border-[var(--color-border-subtle)] bg-[var(--color-surface)] px-2 py-1.5 font-mono text-xs"
            />
            <button
              type="button"
              onClick={() => void pickDirectory()}
              className="shrink-0 rounded border border-[var(--color-border-subtle)] px-3 py-1.5 hover:bg-[var(--color-accent-soft)]"
            >
              更改…
            </button>
            <button
              type="button"
              onClick={() => setDraft((c) => ({ ...c, gameDir: settings.defaultGameDir }))}
              className="shrink-0 rounded border border-[var(--color-border-subtle)] px-3 py-1.5 hover:bg-[var(--color-accent-soft)]"
            >
              恢复默认
            </button>
          </div>
        </div>

        {/* 玩家名 */}
        <div>
          <label
            htmlFor="player-name"
            className="text-[var(--color-text-secondary)]"
          >
            离线玩家名
          </label>
          <input
            id="player-name"
            value={draft.playerName}
            onChange={(event) => setDraft((c) => ({ ...c, playerName: event.target.value }))}
            className="mt-1 w-full rounded border border-[var(--color-border-subtle)] bg-[var(--color-surface)] px-2 py-1.5 text-xs"
          />
        </div>

        {/* 内存 */}
        <div>
          <label htmlFor="memory" className="text-[var(--color-text-secondary)]">
            最大内存（MB）
          </label>
          <input
            id="memory"
            value={draft.memory}
            onChange={(event) => setDraft((c) => ({ ...c, memory: event.target.value }))}
            placeholder="留空 = 官方默认"
            inputMode="numeric"
            className="mt-1 w-full rounded border border-[var(--color-border-subtle)] bg-[var(--color-surface)] px-2 py-1.5 text-xs"
          />
        </div>

        {/* Java */}
        <div className="lg:col-span-2">
          <label htmlFor="java" className="text-[var(--color-text-secondary)]">
            Java 运行时
          </label>
          <div className="mt-1 flex gap-2">
            <select
              id="java"
              value={draft.javaPath}
              onChange={(event) => setDraft((c) => ({ ...c, javaPath: event.target.value }))}
              className="min-w-0 flex-1 rounded border border-[var(--color-border-subtle)] bg-[var(--color-surface)] px-2 py-1.5 text-xs"
            >
              <option value="">自动选择（按版本要求）</option>
              {javaOptions.map((option) => (
                <option key={option.path} value={option.path}>
                  {option.version}（{option.source}）— {option.path}
                </option>
              ))}
            </select>
            <button
              type="button"
              onClick={onRefreshJava}
              className="shrink-0 rounded border border-[var(--color-border-subtle)] px-3 py-1.5 hover:bg-[var(--color-accent-soft)]"
            >
              重新探测
            </button>
            <button
              type="button"
              onClick={() => void pickJava()}
              className="shrink-0 rounded border border-[var(--color-border-subtle)] px-3 py-1.5 hover:bg-[var(--color-accent-soft)]"
            >
              浏览…
            </button>
          </div>
        </div>
      </div>

      {settings.fallbackReason && (
        <p className="mt-2 text-xs text-amber-400">⚠ {settings.fallbackReason}</p>
      )}
      {message && <p className="mt-2 text-xs text-red-400">{message}</p>}
    </section>
  );
}

/** 安装进度条。阶段切换时文字随之变化，避免把两个阶段的百分比混为一谈。 */
export function InstallProgressBar({ progress }: { progress: InstallProgressEvent }) {
  return (
    <div className="mt-2">
      <div className="flex items-center justify-between text-xs text-[var(--color-text-secondary)]">
        <span>
          正在下载{phaseLabel(progress.phase)} {progress.completed}/{progress.total}
        </span>
        <span>{progress.percent}%</span>
      </div>
      <div className="mt-1 h-1.5 overflow-hidden rounded bg-[var(--color-border-subtle)]">
        <div
          className="h-full bg-[var(--color-accent)] transition-[width] duration-200"
          style={{ width: `${progress.percent}%` }}
        />
      </div>
    </div>
  );
}
