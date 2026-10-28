// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { useCallback, useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import {
  beginLogin,
  cancelLogin,
  currentAccount,
  LOGIN_EVENT,
  signOut as invokeSignOut,
  type Account,
  type LoginState,
} from "../api";

/** 登录对话框的阶段。 */
export type LoginPhase =
  | { kind: "idle" }
  | { kind: "starting" }
  | { kind: "code"; userCode: string; verificationUri: string; expiresIn: number }
  | { kind: "failed"; message: string };

/** 账号与登录动作。 */
export interface AccountController {
  /** 当前账号，未登录为 `null`。 */
  account: Account | null;
  /**
   * 是否仍在恢复上次的登录。
   *
   * 这段时间里「没有账号」和「还没查完」看起来一样，界面必须能区分——
   * 否则会先闪一下「离线账户」再跳成「正版账户」。
   */
  restoring: boolean;
  /** 登录对话框阶段。 */
  phase: LoginPhase;
  /** 打开登录对话框并开始设备码流程。 */
  start: () => void;
  /** 关闭对话框并中止后台轮询。 */
  cancel: () => void;
  /** 退出登录。 */
  signOut: () => void;
}

/**
 * 账号状态。
 *
 * 登录的轮询完全在后端，这里只负责把推送来的状态翻译成对话框阶段——前端算时间、
 * 算间隔只会和后端不一致。
 */
export function useAccount(): AccountController {
  const [account, setAccount] = useState<Account | null>(null);
  // 初值为 true：挂载时后端多半还在用刷新令牌换新令牌。
  const [restoring, setRestoring] = useState(true);
  const [phase, setPhase] = useState<LoginPhase>({ kind: "idle" });

  useEffect(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];
    const register = (promise: Promise<UnlistenFn>) => {
      void promise.then((unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        unlisteners.push(unlisten);
      });
    };

    register(
      listen<LoginState>(LOGIN_EVENT, (event) => {
        const state = event.payload;
        switch (state.kind) {
          case "code":
            setPhase({
              kind: "code",
              userCode: state.userCode,
              verificationUri: state.verificationUri,
              expiresIn: state.expiresIn,
            });
            break;
          case "done":
            setAccount(state.account);
            setRestoring(false);
            setPhase({ kind: "idle" });
            break;
          case "failed":
            // 恢复失败也意味着「查完了」：这时说离线账户才是对的。
            setRestoring(false);
            setPhase({ kind: "failed", message: state.message });
            break;
        }
      }),
    );

    // 启动时取一次：账号可能已经在别处登录过（将来持久化之后就是常态）。
    void currentAccount()
      .then((value) => {
        if (!disposed) {
          setAccount(value.account);
          setRestoring(value.restoring);
        }
      })
      .catch(() => {
        // 取不到就按未登录处理，不该因此弹错误——启动器其余功能照常可用。
        if (!disposed) {
          setRestoring(false);
        }
      });

    return () => {
      disposed = true;
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, []);

  const start = useCallback(() => {
    // 用户主动登录：恢复流程已经作废，别再显示「正在恢复」。
    setRestoring(false);
    setPhase({ kind: "starting" });
    void beginLogin().catch((cause: unknown) => {
      setPhase({ kind: "failed", message: String(cause) });
    });
  }, []);

  const cancel = useCallback(() => {
    setPhase({ kind: "idle" });
    // 后端要中止轮询，否则它会一直跑到短码过期。
    void cancelLogin().catch(() => {});
  }, []);

  const signOut = useCallback(() => {
    void invokeSignOut()
      .then(() => {
        setAccount(null);
        setPhase({ kind: "idle" });
      })
      .catch(() => {});
  }, []);

  return { account, restoring, phase, start, cancel, signOut };
}
