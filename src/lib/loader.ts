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
