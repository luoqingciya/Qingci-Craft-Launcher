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

const version = arg("--version");
const installer = arg("--installer");
const signaturePath = arg("--signature");
const baseUrl = arg("--base-url");
const out = arg("--out", { required: false, fallback: "latest.json" });
const notes = arg("--notes", { required: false, fallback: "" });
const platform = arg("--platform", { required: false, fallback: "windows-x86_64" });

const signature = readFileSync(signaturePath, "utf8").trim();
if (signature === "") {
  throw new Error(`${signaturePath} 是空的，签名没有生成？`);
}

// 安装包文件名带空格，必须编码；客户端拿这个 URL 直接下载。
const fileName = basename(installer);
const url = `${baseUrl.replace(/\/+$/, "")}/${encodeURIComponent(fileName)}`;

const manifest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    [platform]: { signature, url },
  },
};

writeFileSync(out, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");

console.log(`latest.json: version=${version} platform=${platform}`);
console.log(`  url=${url}`);
