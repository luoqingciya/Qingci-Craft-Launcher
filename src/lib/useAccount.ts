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
            setPhase({ kind: "idle" });
            break;
          case "failed":
            setPhase({ kind: "failed", message: state.message });
            break;
        }
      }),
    );

    // 启动时取一次：账号可能已经在别处登录过（将来持久化之后就是常态）。
    void currentAccount()
      .then((value) => {
        if (!disposed) {
          setAccount(value);
        }
      })
      .catch(() => {
        // 取不到就按未登录处理，不该因此弹错误——启动器其余功能照常可用。
      });

    return () => {
      disposed = true;
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, []);

  const start = useCallback(() => {
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

  return { account, phase, start, cancel, signOut };
}
