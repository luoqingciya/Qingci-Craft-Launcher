// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

/**
 * 在线更新：检查、下载、安装。
 *
 * 真正的下载与安装交给 tauri-plugin-updater：它按 `latest.json` 里公布的地址下载，
 * 用内置公钥校验 minisign 签名，签名对不上就拒绝安装。所以这里**不做任何版本判断**，
 * 也不自己拼下载地址——一旦绕过插件，「更新」就成了从网上拉一个 exe 直接执行。
 *
 * 这一层只做一件事：把插件的 `Update` 对象翻译成界面好用的形状。
 */

/** 一次可用更新的描述。 */
export interface UpdateInfo {
  /** 新版本号。 */
  version: string;
  /** 更新说明（来自 latest.json 的 notes）。 */
  notes: string | null;
}

/** 已查到、但还没安装的更新。 */
export interface PendingUpdate {
  info: UpdateInfo;
  /**
   * 下载并安装。
   *
   * Windows 上安装会**退出当前应用**（由安装程序接管并自动重启），
   * 所以调用之后不要指望还能更新界面。
   */
  install: (onPercent: (percent: number) => void) => Promise<void>;
  /** 放弃这次更新，释放插件在 Rust 侧占用的资源。 */
  discard: () => Promise<void>;
}

/**
 * 当前平台是否支持自动更新。
 *
 * 两种不支持的情况：
 * 1. 浏览器预览页（`pnpm dev` 打开 preview.html）里没有 Tauri 运行时；
 * 2. **Linux**。清单 `latest.json` 里只有 `windows-x86_64` 一条——Linux 的
 *    AppImage 有 80 MB，不上 COS，只在 GitHub Release 提供，所以没有可供
 *    自动更新下载的地址。这种情况下不该给出一个「点了就报错」的按钮。
 *
 * 平台判断用 UA：WebKitGTK 的 UA 里带 `X11; Linux x86_64`，Windows 是 `Windows NT`。
 */
export function updateSupported(): boolean {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
    return false;
  }
  return !/Linux/i.test(navigator.userAgent);
}

/** 查询更新。返回 `null` 表示已是最新版本。 */
export async function checkForUpdate(): Promise<PendingUpdate | null> {
  const { check } = await import("@tauri-apps/plugin-updater");
  const update = await check();
  if (!update) {
    return null;
  }

  return {
    info: { version: update.version, notes: update.body ?? null },
    install: async (onPercent) => {
      let total = 0;
      let downloaded = 0;

      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
          return;
        }
        if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          // 服务端没给 Content-Length 时算不出百分比：宁可停在 0，也不要乱跳。
          if (total > 0) {
            onPercent(Math.min(100, (downloaded / total) * 100));
          }
          return;
        }
        onPercent(100);
      });
    },
    discard: () => update.close(),
  };
}
