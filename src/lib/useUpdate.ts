// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { useCallback, useEffect, useRef, useState } from "react";

import {
  checkForUpdate,
  updateSupported,
  type PendingUpdate,
  type UpdateInfo,
} from "./update";

/** 更新流程的状态机。 */
export type UpdateState =
  | { kind: "unsupported" }
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "latest" }
  | { kind: "available"; info: UpdateInfo }
  | { kind: "installing"; info: UpdateInfo; percent: number }
  | { kind: "error"; message: string };

/** 更新状态与两个动作，供界面与「关于」标签上的提示点共用。 */
export interface UpdateController {
  state: UpdateState;
  /** 手动检查（会显示进度与错误）。 */
  check: () => void;
  /** 下载并安装当前查到的更新。 */
  install: () => void;
}

/**
 * 更新流程。
 *
 * 启动时静默查一次：查到就提示，查不到（离线、COS 不可达）就什么也不说——
 * 一个「检查更新失败」的红条出现在刚打开的启动器上，只会让人以为程序坏了。
 */
export function useUpdate(): UpdateController {
  // 初始状态就决定好：不支持的平台不该先进 idle 再被 effect 改掉。
  const [state, setState] = useState<UpdateState>(() =>
    updateSupported() ? { kind: "idle" } : { kind: "unsupported" },
  );
  const pending = useRef<PendingUpdate | null>(null);
  // 防重入：用户连点两下「检查更新」不该发出两次请求。
  const busy = useRef(false);

  const run = useCallback(async (silent: boolean) => {
    if (busy.current) {
      return;
    }
    busy.current = true;
    if (!silent) {
      setState({ kind: "checking" });
    }

    try {
      // 上一次查到的更新如果没装，先释放，否则后端会一直握着那份元数据。
      await pending.current?.discard();
      pending.current = null;

      const update = await checkForUpdate();
      pending.current = update;
      setState(update ? { kind: "available", info: update.info } : { kind: "latest" });
    } catch (cause: unknown) {
      setState(silent ? { kind: "idle" } : { kind: "error", message: String(cause) });
    } finally {
      busy.current = false;
    }
  }, []);

  const check = useCallback(() => {
    if (!updateSupported()) {
      return;
    }
    void run(false);
  }, [run]);

  const install = useCallback(() => {
    const update = pending.current;
    if (!update) {
      return;
    }
    setState({ kind: "installing", info: update.info, percent: 0 });
    void update
      .install((percent) => {
        setState({ kind: "installing", info: update.info, percent });
      })
      .catch((cause: unknown) => {
        setState({ kind: "error", message: String(cause) });
      });
  }, []);

  useEffect(() => {
    if (!updateSupported()) {
      return;
    }
    // 「挂载时查一次更新」正是 effect 该做的事（与外部系统同步）。
    // 规则把 run() 里 await 之后的 setState 也算成了「effect 内同步改状态」，
    // 但这里没有任何同步 setState——检查期间状态一直是 idle。
    // oxlint-disable-next-line react/set-state-in-effect
    void run(true);
  }, [run]);

  return { state, check, install };
}
