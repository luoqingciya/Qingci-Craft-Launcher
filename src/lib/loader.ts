// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import type { LoaderKind } from "../api";

/**
 * 把设置里存的加载器种类还原成界面用的值。
 *
 * `loaderKind` 是后加的字段：旧实例没有它，此时按 Fabric 处理——那之前只有 Fabric。
 * 认不出来的值同样退回 Fabric，而不是让界面拿到一个不存在的类型。
 *
 * 抽成纯函数是因为它出过一次真实故障：切换实例后界面没跟着恢复种类，
 * 于是拿 Forge 的版本号去请求 Fabric 的 profile，后端返回 HTTP 400。
 */
export function parseLoaderKind(raw: string | null | undefined): LoaderKind {
  return raw === "forge" || raw === "neoforge" ? raw : "fabric";
}

/**
 * 一次安装失败，是不是「加载器版本不属于当前种类」？
 *
 * 判据是上游回了 **4xx**：说明请求到达了、但那个版本号被拒绝——典型原因是切换
 * 加载器种类时点到了还没刷新的旧列表（Forge 实例记下 Fabric 的 0.20.1 → 404）。
 * 网络错误不算：那种重试即可，不该动用户已经选好的版本。
 */
export function isLoaderVersionRejected(message: string): boolean {
  return /(安装 (?:Forge|NeoForge) 失败|获取 Fabric profile 失败)[\s\S]*HTTP 4\d\d/.test(
    message,
  );
}
