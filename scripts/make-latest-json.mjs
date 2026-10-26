#!/usr/bin/env node
// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

// 生成 Tauri updater 读取的 latest.json。
//
// 字段格式由 tauri-plugin-updater 的 `RemoteRelease` 决定：
//   { version, notes?, pub_date?, platforms: { "<target>": { url, signature } } }
//
// 这里的 signature 是 `.sig` 文件的**原文**（一整行 base64），不是解码后的内容。
//
// 每个平台一项 `--platform 目标=安装包=签名文件`：
//   node scripts/make-latest-json.mjs --version 0.1.2 \
//     --base-url https://qcl.luoqingciya.cn/releases \
//     --platform "windows-x86_64=out/x.exe=out/x.exe.sig" \
//     --platform "linux-x86_64=out/x.AppImage=out/x.AppImage.sig" \
//     --out out/latest.json
//
// 用 `=` 而不是 `:` 分隔：Windows 路径里的盘符是冒号。

import { readFileSync, writeFileSync } from "node:fs";
import { basename } from "node:path";

function arg(name, { required = true, fallback = undefined } = {}) {
  const index = process.argv.indexOf(name);
  if (index === -1) {
    if (required) {
      throw new Error(`缺少参数 ${name}`);
    }
    return fallback;
  }
  const value = process.argv[index + 1];
  if (value === undefined || value.startsWith("--")) {
    throw new Error(`参数 ${name} 没有值`);
  }
  return value;
}

function args(name) {
  const values = [];
  process.argv.forEach((item, index) => {
    const next = process.argv[index + 1];
    if (item === name && next !== undefined) {
      values.push(next);
    }
  });
  return values;
}

const version = arg("--version");
const baseUrl = arg("--base-url");
const out = arg("--out", { required: false, fallback: "latest.json" });
const notes = arg("--notes", { required: false, fallback: "" });
const specs = args("--platform");

if (specs.length === 0) {
  throw new Error("至少要有一个 --platform 目标=安装包=签名文件");
}

const platforms = {};
for (const spec of specs) {
  const parts = spec.split("=");
  if (parts.length !== 3) {
    throw new Error(`--platform 需要 目标=安装包=签名文件，收到：${spec}`);
  }
  const [target, installer, signaturePath] = parts;

  const signature = readFileSync(signaturePath, "utf8").trim();
  if (signature === "") {
    throw new Error(`${signaturePath} 是空的，签名没有生成？`);
  }

  // 安装包文件名可能带空格，必须编码；客户端拿这个 URL 直接下载。
  const fileName = basename(installer);
  platforms[target] = {
    signature,
    url: `${baseUrl.replace(/\/+$/, "")}/${encodeURIComponent(fileName)}`,
  };
}

const manifest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms,
};

writeFileSync(out, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");

console.log(`latest.json: version=${version}`);
for (const [target, entry] of Object.entries(platforms)) {
  console.log(`  ${target} → ${entry.url}`);
}
