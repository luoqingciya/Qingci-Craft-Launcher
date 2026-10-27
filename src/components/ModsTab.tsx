// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { useCallback, useEffect, useState } from "react";

import {
  checkModUpdates,
  installMod,
  listModCategories,
  listMods,
  removeMod,
  searchMods,
  updateMod,
  type ModCategory,
  type ModHit,
  type ModUpdate,
} from "../api";

/**
 * 模组页：从 Modrinth 搜索、安装、卸载。
 *
 * 自己持有数据而不是由 `App` 传入：模组是独立的一块功能，
 * 它的状态（查询、结果、安装中）与启动流程无关，放上去只会让 App 更臃肿。
 *
 * 依赖解析刻意没做——Modrinth 的依赖关系不保证完整，猜错了会让人装上一个
 * 启动即崩的组合；缺依赖时游戏自己的日志说得更清楚。
 */

/** 按下载量降序。 */
const byDownloads = (a: ModHit, b: ModHit): number => b.downloads - a.downloads;

/** 排序方式。相关度是 Modrinth 的原始顺序，下载量是按热度重排。 */
const SORTS: { id: "relevance" | "downloads"; label: string }[] = [
  { id: "relevance", label: "相关度" },
  { id: "downloads", label: "下载量" },
];

/** 空的「本次已装」集合：作用域不匹配时复用它，避免每次渲染都 new 一个。 */
const EMPTY_IDS: ReadonlySet<string> = new Set();

export function ModsTab({
  version,
  loader,
}: {
  version: string | null;
  /** 加载器版本；空串表示原版。 */
  loader: string;
}) {
  const [query, setQuery] = useState("");
  const [categories, setCategories] = useState<ModCategory[]>([]);
  const [selectedCategories, setSelectedCategories] = useState<string[]>([]);
  // 中文输入会被换成英文关键词，把实际用的词显示出来，避免「我明明搜的是钠」的困惑。
  const [resolvedQuery, setResolvedQuery] = useState<string | null>(null);
  const [results, setResults] = useState<ModHit[]>([]);
  const [installed, setInstalled] = useState<string[]>([]);
  const [searching, setSearching] = useState(false);
  const [installingId, setInstallingId] = useState<string | null>(null);
  // 删除是两步的：先点「删除」进入待确认，再点「确认删除」才真的动文件。
  const [confirming, setConfirming] = useState<string | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);
  const [updates, setUpdates] = useState<ModUpdate[]>([]);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [updatingFile, setUpdatingFile] = useState<string | null>(null);
  // 搜索与已装分成两页：挤在一页里，谁都不够用。
  const [view, setView] = useState<"search" | "installed">("search");
  // 分类默认只显示常用的几个，其余收进「更多」。
  const [showAllCategories, setShowAllCategories] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [searched, setSearched] = useState(false);
  const [sort, setSort] = useState<"relevance" | "downloads">("relevance");
  // 本次会话装过哪些项目。真正的「已安装」需要后端把 jar 文件名映射回项目 id，
  // 现在做不到；先只记本会话，至少避免同一个模组被连点两次。
  const [installedHere, setInstalledHere] = useState<{ scope: string; ids: string[] }>({
    scope: "",
    ids: [],
  });

  const modLoader = loader ? "fabric" : "";
  // 版本或加载器一换，之前记下的「装过」就不再适用了。
  const scope = `${version ?? ""}|${loader}`;
  const installedIds = installedHere.scope === scope ? new Set(installedHere.ids) : EMPTY_IDS;
  // 用副本 + sort 而不是 `toSorted`：后者是 ES2023，macOS 12 上的 Safari 15 没有，
  // 而项目支持 macOS 12+。tsconfig 的 target 也是 ES2022。
  // oxlint-disable-next-line unicorn/no-array-sort
  const sortedResults = sort === "downloads" ? [...results].sort(byDownloads) : results;

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
    listModCategories()
      .then((list) => {
        if (!cancelled) {
          setCategories(list);
        }
      })
      .catch(() => {
        // 拉不到分类就不显示筛选条，不影响搜索。
        if (!cancelled) {
          setCategories([]);
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

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

  async function checkUpdates() {
    if (!version) {
      return;
    }
    setCheckingUpdates(true);
    setError(null);
    setNotice(null);
    try {
      const found = await checkModUpdates(version, modLoader);
      setUpdates(found);
      setNotice(found.length === 0 ? "所有模组都是最新版本" : `${found.length} 个模组有新版本`);
    } catch (cause: unknown) {
      setError(String(cause));
    } finally {
      setCheckingUpdates(false);
    }
  }

  async function applyUpdate(update: ModUpdate) {
    if (!version) {
      return;
    }
    setUpdatingFile(update.fileName);
    setError(null);
    setNotice(null);
    try {
      await updateMod(update.projectId, update.fileName, version, modLoader);
      setNotice(`已更新到 ${update.latestVersion}`);
      setUpdates((current) => current.filter((item) => item.fileName !== update.fileName));
      await refreshInstalled();
    } catch (cause: unknown) {
      setError(String(cause));
    } finally {
      setUpdatingFile(null);
    }
  }

  function toggleCategory(id: string) {
    setSelectedCategories((current) =>
      current.includes(id) ? current.filter((item) => item !== id) : [...current, id]
    );
  }

  async function submit() {
    // 只选分类、不输关键词也是有效搜索（按类别浏览）。
    if (!version || (!query.trim() && selectedCategories.length === 0)) {
      return;
    }
    setSearching(true);
    setError(null);
    setNotice(null);
    try {
      const outcome = await searchMods(
        query.trim(),
        version,
        modLoader,
        selectedCategories
      );
      setResults(outcome.hits);
      setResolvedQuery(outcome.query === query.trim() ? null : outcome.query);
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
      // 记下「这次装过」，让按钮变成不可重复点的「已安装」。
      const known = installedHere.scope === scope ? installedHere.ids : [];
      setInstalledHere({ scope, ids: [...known, hit.projectId] });
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

      <div role="tablist" aria-label="模组视图" className="flex gap-1 border-b border-[var(--color-border-subtle)]">
        <SubTab active={view === "search"} onClick={() => setView("search")}>
          搜索
        </SubTab>
        <SubTab active={view === "installed"} onClick={() => setView("installed")}>
          已安装 {installed.length}
          {updates.length > 0 && (
            <span className="ml-1.5 rounded bg-[var(--color-warning)]/15 px-1.5 py-0.5 text-[10px] text-[var(--color-warning)]">
              {updates.length} 可更新
            </span>
          )}
        </SubTab>
      </div>

      {/* 消息放在两页之外：检查更新的结果、删除失败的原因，在哪个页都要能看到。 */}
      {notice && <p className="text-[12px] text-[var(--color-success)]">{notice}</p>}
      {error && <p className="text-[12px] text-[var(--color-danger)]">{error}</p>}

      {view === "search" && (
        <>
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
          placeholder="搜索模组，如 sodium / 钠（也可只点下面的分类）"
          spellCheck={false}
          className="min-w-0 flex-1 rounded-xl bg-[var(--color-surface-raised)] px-3.5 py-2.5 text-[13px] ring-1 ring-inset ring-[var(--color-border-subtle)] outline-none placeholder:text-[var(--color-text-muted)] focus:ring-[var(--color-accent)]"
        />
        <button
          type="submit"
          disabled={searching || (!query.trim() && selectedCategories.length === 0)}
          className="rounded-xl bg-[var(--color-accent)] px-5 text-[13px] font-medium text-white transition-colors hover:bg-[var(--color-accent-strong)] disabled:opacity-40"
        >
          {searching ? "搜索中…" : "搜索"}
        </button>
      </form>

      {categories.length > 0 && (
        <div
          className="flex gap-1.5 overflow-x-auto pb-0.5"
          role="group"
          aria-label="按分类筛选"
        >
          {(showAllCategories ? categories : categories.slice(0, 8)).map((category) => {
            const active = selectedCategories.includes(category.id);
            return (
              <button
                key={category.id}
                type="button"
                aria-pressed={active}
                onClick={() => toggleCategory(category.id)}
                className={`shrink-0 whitespace-nowrap rounded-full px-3 py-1 text-[12px] transition-colors ${
                  active
                    ? "bg-[var(--color-accent-soft)] text-[var(--color-accent-text)] ring-1 ring-inset ring-[var(--color-accent)]"
                    : "text-[var(--color-text-secondary)] ring-1 ring-inset ring-[var(--color-border-subtle)] hover:bg-[var(--color-surface-hover)]"
                }`}
              >
                {category.label}
              </button>
            );
          })}

          {categories.length > 8 && (
            <button
              type="button"
              onClick={() => setShowAllCategories((current) => !current)}
              className="shrink-0 whitespace-nowrap rounded-full px-3 py-1 text-[12px] text-[var(--color-text-muted)] hover:bg-[var(--color-surface-hover)]"
            >
              {showAllCategories ? "收起" : `更多 ${categories.length - 8}`}
            </button>
          )}
        </div>
      )}

      {resolvedQuery && (
        <p className="text-[11px] text-[var(--color-text-muted)]">
          已按 <span className="font-mono">{resolvedQuery}</span> 搜索（Modrinth 只索引英文）
        </p>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto">
        {searched && results.length === 0 && !searching && (
          <p className="py-6 text-[13px] text-[var(--color-text-muted)]">
            没有找到适配该版本的模组。
          </p>
        )}

        {searched && results.length > 0 && (
          <div className="flex items-center justify-between gap-2 pb-1">
            <span className="text-[11px] text-[var(--color-text-muted)]">
              <span className="tabular-nums">{results.length}</span> 个结果 · 适配{" "}
              <span className="font-mono">{version}</span> ·{" "}
              {loader ? `Fabric ${loader}` : "原版"}
            </span>
            <div role="group" aria-label="排序" className="flex items-center gap-0.5">
              {SORTS.map((item) => {
                const active = sort === item.id;
                return (
                  <button
                    key={item.id}
                    type="button"
                    aria-pressed={active}
                    onClick={() => setSort(item.id)}
                    className={`rounded-md px-2 py-0.5 text-[11px] transition-colors ${
                      active
                        ? "bg-[var(--color-accent-soft)] font-medium text-[var(--color-accent-text)]"
                        : "text-[var(--color-text-muted)] hover:bg-[var(--color-surface-hover)] hover:text-[var(--color-text-secondary)]"
                    }`}
                  >
                    {item.label}
                  </button>
                );
              })}
            </div>
          </div>
        )}

        <ul className="flex flex-col gap-2">
          {sortedResults.map((hit) => (
            <li
              key={hit.projectId}
              className="flex items-start gap-3 rounded-xl bg-[var(--color-surface-raised)] p-3.5 ring-1 ring-inset ring-[var(--color-border-subtle)]"
            >
              <ModIcon url={hit.iconUrl} title={hit.title} />
              <div className="min-w-0 flex-1">
                <div className="flex items-baseline gap-2">
                  <span className="truncate text-[14px] font-medium">
                    {hit.zhName ?? hit.title}
                  </span>
                  {hit.zhName && (
                    <span className="shrink-0 truncate text-[12px] text-[var(--color-text-muted)]">
                      {hit.title}
                    </span>
                  )}
                  <span className="shrink-0 text-[11px] text-[var(--color-text-muted)]">
                    {hit.author}
                  </span>
                </div>
                <p className="mt-1 line-clamp-2 text-[12px] leading-relaxed text-[var(--color-text-secondary)]">
                  {hit.description}
                </p>
                <p className="mt-1 flex items-center gap-3 text-[11px] text-[var(--color-text-muted)]">
                  <span className="tabular-nums">{formatDownloads(hit.downloads)} 次下载</span>
                  {/* 描述保持英文：机翻长文本的术语质量不稳，不如让用户看原页 */}
                  <button
                    type="button"
                    onClick={() => void openModrinthPage(hit.slug)}
                    className="text-[var(--color-accent-text)] hover:underline"
                  >
                    在 Modrinth 查看
                  </button>
                </p>
              </div>

              <button
                type="button"
                onClick={() => void install(hit)}
                disabled={installingId !== null || installedIds.has(hit.projectId)}
                className="h-9 shrink-0 rounded-lg px-3.5 text-[12px] text-[var(--color-text-primary)] ring-1 ring-inset ring-[var(--color-border-strong)] transition-colors hover:bg-[var(--color-surface-hover)] disabled:opacity-40"
              >
                {installingId === hit.projectId
                  ? "安装中…"
                  : installedIds.has(hit.projectId)
                    ? "已安装"
                    : "安装"}
              </button>
            </li>
          ))}
        </ul>
      </div>

        </>
      )}

      {view === "installed" && (
      <section className="flex min-h-0 flex-1 flex-col rounded-xl bg-[var(--color-surface-raised)] p-3.5 ring-1 ring-inset ring-[var(--color-border-subtle)]">
        <div className="flex shrink-0 items-center justify-between gap-2">
          <h3 className="text-[12px] uppercase tracking-wider text-[var(--color-text-muted)]">
            已安装 {installed.length} 个
          </h3>
          <button
            type="button"
            onClick={() => void checkUpdates()}
            disabled={checkingUpdates || installed.length === 0}
            className="shrink-0 rounded-md px-2 py-1 text-[11px] text-[var(--color-accent-text)] hover:bg-[var(--color-accent-soft)] disabled:opacity-40"
          >
            {checkingUpdates ? "检查中…" : "检查更新"}
          </button>
        </div>
        {installed.length === 0 ? (
          <p className="mt-2 text-[12px] text-[var(--color-text-muted)]">
            还没有安装任何模组。
          </p>
        ) : (
          <ul className="mt-2 flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto">
            {installed.map((name) => {
              const update = updates.find((item) => item.fileName === name);
              return (
              <li key={name} className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-[var(--color-text-secondary)]">
                  {name}
                </span>

                {update && (
                  <>
                    <span className="shrink-0 text-[11px] text-[var(--color-warning)]">
                      {update.currentVersion} → {update.latestVersion}
                    </span>
                    <button
                      type="button"
                      onClick={() => void applyUpdate(update)}
                      disabled={updatingFile !== null}
                      className="shrink-0 rounded-md px-2 py-1 text-[11px] text-[var(--color-accent-text)] ring-1 ring-inset ring-[var(--color-accent)]/40 hover:bg-[var(--color-accent-soft)] disabled:opacity-40"
                    >
                      {updatingFile === name ? "更新中…" : "更新"}
                    </button>
                  </>
                )}

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
              );
            })}
          </ul>
        )}
      </section>
      )}
    </div>
  );
}


/** 在系统浏览器里打开 Modrinth 项目页。浏览器预览页里退回 window.open。 */
async function openModrinthPage(slug: string): Promise<void> {
  const url = `https://modrinth.com/mod/${slug}`;
  try {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } catch {
    window.open(url, "_blank", "noreferrer");
  }
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

/** 模组页内部的子标签（搜索 / 已安装）。 */
function SubTab({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={`-mb-px flex items-center border-b-2 px-3 py-2 text-[13px] transition-colors ${
        active
          ? "border-[var(--color-accent)] text-[var(--color-text-primary)]"
          : "border-transparent text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]"
      }`}
    >
      {children}
    </button>
  );
}
