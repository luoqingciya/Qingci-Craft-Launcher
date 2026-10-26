// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { useCallback, useEffect, useState } from "react";

import { installMod, listMods, removeMod, searchMods, type ModHit } from "../api";

/**
 * 模组页：从 Modrinth 搜索、安装、卸载。
 *
 * 自己持有数据而不是由 `App` 传入：模组是独立的一块功能，
 * 它的状态（查询、结果、安装中）与启动流程无关，放上去只会让 App 更臃肿。
 *
 * 依赖解析刻意没做——Modrinth 的依赖关系不保证完整，猜错了会让人装上一个
 * 启动即崩的组合；缺依赖时游戏自己的日志说得更清楚。
 */
export function ModsTab({
  version,
  loader,
}: {
  version: string | null;
  /** 加载器版本；空串表示原版。 */
  loader: string;
}) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<ModHit[]>([]);
  const [installed, setInstalled] = useState<string[]>([]);
  const [searching, setSearching] = useState(false);
  const [installingId, setInstallingId] = useState<string | null>(null);
  // 删除是两步的：先点「删除」进入待确认，再点「确认删除」才真的动文件。
  const [confirming, setConfirming] = useState<string | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [searched, setSearched] = useState(false);

  const modLoader = loader ? "fabric" : "";

  const refreshInstalled = useCallback(async () => {
    try {
      setInstalled(await listMods());
    } catch {
      // 读不到列表不影响搜索，界面上就是「还没有安装模组」。
      setInstalled([]);
    }
  }, []);

  // 首次进入时读一次已装列表。setState 放在 .then 里而不是 effect 体内：
  // effect 里同步 setState 会多渲染一轮（lint 的 set-state-in-effect 也会拦）。
  useEffect(() => {
    let cancelled = false;
    listMods()
      .then((names) => {
        if (!cancelled) {
          setInstalled(names);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setInstalled([]);
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function submit() {
    if (!version || !query.trim()) {
      return;
    }
    setSearching(true);
    setError(null);
    setNotice(null);
    try {
      setResults(await searchMods(query.trim(), version, modLoader));
      setSearched(true);
    } catch (cause: unknown) {
      setError(String(cause));
    } finally {
      setSearching(false);
    }
  }

  async function install(hit: ModHit) {
    if (!version) {
      return;
    }
    setInstallingId(hit.projectId);
    setError(null);
    setNotice(null);
    try {
      const paths = await installMod(hit.projectId, version, modLoader);
      setNotice(
        paths.length > 1
          ? `已安装 ${paths.length} 个文件（含依赖）`
          : `已安装到 ${paths[0] ?? "mods/"}`,
      );
      await refreshInstalled();
    } catch (cause: unknown) {
      setError(String(cause));
    } finally {
      setInstallingId(null);
    }
  }

  async function remove(filename: string) {
    setRemoving(filename);
    setError(null);
    setNotice(null);
    try {
      await removeMod(filename);
      setNotice(`已删除 ${filename}`);
      await refreshInstalled();
    } catch (cause: unknown) {
      setError(String(cause));
    } finally {
      setRemoving(null);
      setConfirming(null);
    }
  }

  // 没选版本时搜索框点了也没反应；原版则装了不会被加载。两种情况都先讲清楚。
  if (!version || !loader) {
    return (
      <div className="mx-auto w-full max-w-[720px] px-6 py-10">
        <h2 className="text-[15px] font-medium">模组</h2>
        <p className="mt-2 text-[13px] leading-relaxed text-[var(--color-text-secondary)]">
          {version
            ? "当前是原版，模组不会被加载。请先在「启动」页把加载器换成 Fabric。"
            : "先选择一个版本，并给它配上 Fabric 加载器。"}
        </p>
      </div>
    );
  }

  return (
    <div className="mx-auto flex h-full w-full max-w-[760px] flex-col gap-4 px-6 py-6">
      <div>
        <h2 className="text-[15px] font-medium">模组</h2>
        <p className="mt-1 text-[12px] text-[var(--color-text-secondary)]">
          为 {version} · Fabric {loader} 搜索 Modrinth 上的模组
        </p>
      </div>

      <form
        className="flex gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="搜索模组，如 sodium / lithium"
          spellCheck={false}
          className="min-w-0 flex-1 rounded-xl bg-[var(--color-surface-raised)] px-3.5 py-2.5 text-[13px] ring-1 ring-inset ring-[var(--color-border-subtle)] outline-none placeholder:text-[var(--color-text-muted)] focus:ring-[var(--color-accent)]"
        />
        <button
          type="submit"
          disabled={searching || !query.trim()}
          className="rounded-xl bg-[var(--color-accent)] px-5 text-[13px] font-medium text-white transition-colors hover:bg-[var(--color-accent-strong)] disabled:opacity-40"
        >
          {searching ? "搜索中…" : "搜索"}
        </button>
      </form>

      {notice && <p className="text-[12px] text-[var(--color-success)]">{notice}</p>}
      {error && <p className="text-[12px] text-[var(--color-danger)]">{error}</p>}

      <div className="min-h-0 flex-1 overflow-y-auto">
        {searched && results.length === 0 && !searching && (
          <p className="py-6 text-[13px] text-[var(--color-text-muted)]">
            没有找到适配该版本的模组。
          </p>
        )}

        <ul className="flex flex-col gap-2">
          {results.map((hit) => (
            <li
              key={hit.projectId}
              className="flex items-start gap-3 rounded-xl bg-[var(--color-surface-raised)] p-3.5 ring-1 ring-inset ring-[var(--color-border-subtle)]"
            >
              <ModIcon url={hit.iconUrl} title={hit.title} />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="truncate text-[14px] font-medium">{hit.title}</span>
                  <span className="shrink-0 text-[11px] text-[var(--color-text-muted)]">
                    {hit.author}
                  </span>
                </div>
                <p className="mt-1 line-clamp-2 text-[12px] leading-relaxed text-[var(--color-text-secondary)]">
                  {hit.description}
                </p>
                <p className="mt-1 text-[11px] tabular-nums text-[var(--color-text-muted)]">
                  {formatDownloads(hit.downloads)} 次下载
                </p>
              </div>

              <button
                type="button"
                onClick={() => void install(hit)}
                disabled={installingId !== null}
                className="h-9 shrink-0 rounded-lg px-3.5 text-[12px] text-[var(--color-text-primary)] ring-1 ring-inset ring-[var(--color-border-strong)] transition-colors hover:bg-[var(--color-surface-hover)] disabled:opacity-40"
              >
                {installingId === hit.projectId ? "安装中…" : "安装"}
              </button>
            </li>
          ))}
        </ul>
      </div>

      <section className="shrink-0 rounded-xl bg-[var(--color-surface-raised)] p-3.5 ring-1 ring-inset ring-[var(--color-border-subtle)]">
        <h3 className="text-[12px] uppercase tracking-wider text-[var(--color-text-muted)]">
          已安装 {installed.length} 个
        </h3>
        {installed.length === 0 ? (
          <p className="mt-2 text-[12px] text-[var(--color-text-muted)]">
            还没有安装任何模组。
          </p>
        ) : (
          <ul className="mt-2 flex flex-col gap-1">
            {installed.map((name) => (
              <li key={name} className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-[var(--color-text-secondary)]">
                  {name}
                </span>

                {confirming === name ? (
                  <>
                    <button
                      type="button"
                      onClick={() => void remove(name)}
                      disabled={removing === name}
                      className="shrink-0 rounded-md px-2 py-1 text-[11px] text-[var(--color-danger)] ring-1 ring-inset ring-[var(--color-danger)]/40 hover:bg-[var(--color-danger)]/10 disabled:opacity-40"
                    >
                      {removing === name ? "删除中…" : "确认删除"}
                    </button>
                    <button
                      type="button"
                      onClick={() => setConfirming(null)}
                      className="shrink-0 rounded-md px-2 py-1 text-[11px] text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-hover)]"
                    >
                      取消
                    </button>
                  </>
                ) : (
                  <button
                    type="button"
                    onClick={() => setConfirming(name)}
                    className="shrink-0 rounded-md px-2 py-1 text-[11px] text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-danger)]"
                  >
                    删除
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

/** 模组图标。没有图标时用标题首字占位，避免行高塌陷。 */
function ModIcon({ url, title }: { url: string | null; title: string }) {
  const [failed, setFailed] = useState(false);

  if (!url || failed) {
    return (
      <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-[var(--color-surface-hover)] text-[15px] text-[var(--color-text-muted)]">
        {title.trim().charAt(0) || "?"}
      </span>
    );
  }

  return (
    <img
      src={url}
      alt=""
      loading="lazy"
      onError={() => setFailed(true)}
      className="h-10 w-10 shrink-0 rounded-lg object-cover"
    />
  );
}

/** 下载量：一万以上用「万」，避免一长串数字。 */
export function formatDownloads(count: number): string {
  if (count >= 100_000_000) {
    return `${(count / 100_000_000).toFixed(1)} 亿`;
  }
  if (count >= 10_000) {
    return `${(count / 10_000).toFixed(1)} 万`;
  }
  return String(count);
}
