// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

import { version } from "../../package.json";
// 用已生成好的 128px 图标而不是 929 KB 的源文件：界面里最大也只显示 56px。
import iconUrl from "../../src-tauri/icons/128x128.png";
import type { UpdateState } from "../lib/useUpdate";
import { ExternalLink } from "./ExternalLink";

/**
 * 下载页地址：便携版用户要自己去换 exe。
 *
 * 指向 GitHub Releases 而不是 COS 的目录：COS 的 `/releases/` 在没开静态网站
 * 或对象列表时访问会 403，而且那里也没有版本说明。
 */
const DOWNLOAD_URL = "https://github.com/luoqingciya/Qingci-Craft-Launcher/releases/latest";

/**
 * 关于页。
 *
 * 这里的信息都是**用户需要知道、但主界面放不下**的内容：版本号、更新、
 * 许可证、以及「这不是官方启动器」的免责声明。放在标签页里而不是弹窗里，
 * 是因为它可能被截图、被复制，弹窗容易被误关。
 */
export function AboutPanel({
  update,
  portable,
  onCheck,
  onInstall,
}: {
  update: UpdateState;
  /** 便携版无法自更新（Windows 不允许替换正在运行的 exe）。 */
  portable: boolean;
  onCheck: () => void;
  onInstall: () => void;
}) {
  return (
    <div className="mx-auto flex w-full max-w-[620px] flex-col gap-4 px-6 py-8">
      <div className="flex items-center gap-4">
        <img src={iconUrl} alt="" className="h-14 w-14 rounded-xl" />
        <div>
          <div className="text-[16px] font-semibold">清辞</div>
          <div className="text-[13px] text-[var(--color-text-secondary)]">
            Qingci Craft Launcher · v{version}
          </div>
        </div>
      </div>

      <p className="text-[13px] leading-relaxed text-[var(--color-text-secondary)]">
        基于 Tauri 2 与 Rust 的轻量级 Minecraft Java 版启动器。
        版本、依赖库与 JVM 参数全部从官方清单推导，不硬编码任何版本列表。
      </p>

      <Section title="更新">
        <UpdateRow update={update} portable={portable} onCheck={onCheck} onInstall={onInstall} />
      </Section>

      <Section title="链接">
        <LinkRow href="https://github.com/luoqingciya/Qingci-Craft-Launcher">
          源代码与问题反馈
        </LinkRow>
        <LinkRow href="https://github.com/luoqingciya/Qingci-Craft-Launcher/releases">
          下载最新版本
        </LinkRow>
      </Section>

      <Section title="许可证">
        <p className="text-[13px] leading-relaxed text-[var(--color-text-secondary)]">
          GPL-3.0，并附有 <strong className="text-[var(--color-text-primary)]">GPLv3 第 7 条附加条款</strong>：
          分发时须保留版权与许可声明；分发修改版须更换名称。
          原样分发未经修改的副本可以保留原名。
        </p>
      </Section>

      <Section title="免责声明">
        <p className="text-[13px] leading-relaxed text-[var(--color-text-secondary)]">
          本项目为非官方启动器，与 Mojang Studios、Microsoft 无任何隶属关系。
          "Minecraft" 是 Mojang Studios 的商标。本项目不分发任何游戏本体、库文件或资源文件，
          全部内容在运行时从官方公开端点获取。
        </p>
      </Section>
    </div>
  );
}

/**
 * 更新状态与操作。
 *
 * 状态机在 `useUpdate` 里，这里只负责把每种状态翻译成一句话和一个按钮：
 * 同一时刻只该有一个明确的下一步动作。
 */
function UpdateRow({
  update,
  portable,
  onCheck,
  onInstall,
}: {
  update: UpdateState;
  portable: boolean;
  onCheck: () => void;
  onInstall: () => void;
}) {
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-3">
        <span className="text-[13px] text-[var(--color-text-secondary)]">
          {headline(update, portable)}
        </span>
        {action(update, portable, onCheck, onInstall)}
      </div>

      {update.kind === "available" && update.info.notes && (
        <p className="whitespace-pre-wrap text-[12px] leading-relaxed text-[var(--color-text-muted)]">
          {update.info.notes}
        </p>
      )}

      {update.kind === "installing" && (
        <div
          className="h-1.5 w-full overflow-hidden rounded-full bg-[var(--color-surface-hover)]"
          role="progressbar"
          aria-valuenow={Math.round(update.percent)}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-label="更新下载进度"
        >
          <div
            className="h-full rounded-full bg-[var(--color-accent)] transition-[width]"
            style={{ width: `${Math.max(2, update.percent)}%` }}
          />
        </div>
      )}

      {update.kind === "available" && portable && (
        <p className="text-[12px] text-[var(--color-text-muted)]">
          当前是便携版，无法自动替换正在运行的程序，请到 <LinkRow href={DOWNLOAD_URL}>下载页</LinkRow>{" "}
          换用新版。
        </p>
      )}

      {update.kind === "unsupported" && (
        <p className="text-[12px] text-[var(--color-text-muted)]">
          Linux 版的安装包不放在更新服务器上，请到下载页获取新版本。
        </p>
      )}

      {update.kind === "error" && (
        <p className="break-all text-[12px] text-[var(--color-danger)]">{update.message}</p>
      )}
    </div>
  );
}

function headline(update: UpdateState, portable: boolean): string {
  switch (update.kind) {
    case "unsupported":
      // Linux：清单里没有对应平台的条目，所以压根没有可自动下载的更新。
      return "当前平台不支持自动更新";
    case "checking":
      return "正在检查更新…";
    case "latest":
      return "已是最新版本";
    case "available":
      return portable
        ? `发现新版本 v${update.info.version}（便携版需手动替换）`
        : `发现新版本 v${update.info.version}`;
    case "installing":
      // 装完之后进程会被安装程序接管，先说清楚，免得用户以为程序自己关了。
      return update.percent >= 100
        ? "正在安装，完成后应用会自动重启"
        : `正在下载更新 ${Math.round(update.percent)}%`;
    case "error":
      return "检查更新失败";
    default:
      return "启动时会自动检查一次";
  }
}

function action(
  update: UpdateState,
  portable: boolean,
  onCheck: () => void,
  onInstall: () => void,
) {
  if (update.kind === "installing") {
    return null;
  }
  if (update.kind === "unsupported") {
    // 给个出口而不是一个点不出结果的按钮。
    return <LinkRow href={DOWNLOAD_URL}>去下载</LinkRow>;
  }
  if (update.kind === "available") {
    return portable ? (
      <LinkRow href={DOWNLOAD_URL}>去下载</LinkRow>
    ) : (
      <button type="button" onClick={onInstall} className={BUTTON}>
        下载并安装
      </button>
    );
  }
  return (
    <button
      type="button"
      onClick={onCheck}
      disabled={update.kind === "checking"}
      className={BUTTON}
    >
      {update.kind === "checking" ? "检查中…" : update.kind === "error" ? "重试" : "检查更新"}
    </button>
  );
}

const BUTTON =
  "shrink-0 rounded-lg px-3 py-1.5 text-[12px] text-[var(--color-accent-text)] ring-1 ring-inset ring-[var(--color-border-strong)] transition-colors hover:bg-[var(--color-surface-hover)] disabled:cursor-not-allowed disabled:opacity-50";

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="rounded-xl bg-[var(--color-surface-raised)] p-4 ring-1 ring-inset ring-[var(--color-border-subtle)]">
      <h3 className="mb-2 text-[12px] font-medium uppercase tracking-wider text-[var(--color-text-muted)]">
        {title}
      </h3>
      <div className="flex flex-col gap-1.5">{children}</div>
    </section>
  );
}

/**
 * 行内链接。
 *
 * 打开逻辑抽到了 [`ExternalLink`]——登录对话框也要用它，两处各写一份
 * opener 插件调用迟早会漂移。
 */
function LinkRow({ href, children }: { href: string; children: React.ReactNode }) {
  return <ExternalLink href={href}>{children}</ExternalLink>;
}
