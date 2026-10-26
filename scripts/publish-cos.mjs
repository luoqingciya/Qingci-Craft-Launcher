#!/usr/bin/env node
// Copyright (C) 2026 luoqingci
// SPDX-License-Identifier: GPL-3.0-only
// 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md

// 把发布产物推到腾讯云 COS。
//
// 用法（凭据从环境变量读，不进命令行——命令行会被记进 CI 日志）：
//   TENCENT_SECRET_ID=... TENCENT_SECRET_KEY=... \
//   node scripts/publish-cos.mjs --bucket qcl-1331847184 --region ap-guangzhou \
//        --prefix releases --file out/QingciCraftLauncher_0.1.1_x64-setup.exe \
//        --file out/latest.json
//
// `--dry-run` 只打印将要上传的对象，不联网、不需要凭据。

import { createReadStream, statSync } from "node:fs";
import { basename } from "node:path";

import COS from "cos-nodejs-sdk-v5";

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
    if (item === name && process.argv[index + 1]) {
      values.push(process.argv[index + 1]);
    }
  });
  return values;
}

const bucket = arg("--bucket");
const region = arg("--region");
const prefix = arg("--prefix", { required: false, fallback: "" }).replace(/^\/+|\/+$/g, "");
const files = args("--file");
const dryRun = process.argv.includes("--dry-run");

if (files.length === 0) {
  throw new Error("至少要有一个 --file");
}

/** 更新清单必须最后传：它一旦可读，客户端就会照着里面的地址去下载。 */
const MANIFEST_NAME = "latest.json";

const ordered = files.toSorted((left, right) => {
  const leftManifest = basename(left) === MANIFEST_NAME ? 1 : 0;
  const rightManifest = basename(right) === MANIFEST_NAME ? 1 : 0;
  return leftManifest - rightManifest;
});

function contentType(name) {
  if (name.endsWith(".json")) return "application/json; charset=utf-8";
  if (name.endsWith(".sig")) return "text/plain; charset=utf-8";
  return "application/octet-stream";
}

/** 安装包按版本号命名、内容永不变化，可以长缓存；清单必须每次回源。 */
function cacheControl(name) {
  return name === MANIFEST_NAME ? "no-cache" : "public, max-age=31536000, immutable";
}

function keyOf(local) {
  return prefix === "" ? basename(local) : `${prefix}/${basename(local)}`;
}

const objects = ordered.map((local) => {
  const size = statSync(local).size;
  if (size === 0) {
    throw new Error(`${local} 是空文件，拒绝上传`);
  }
  return { local, key: keyOf(local), size };
});

for (const object of objects) {
  console.log(`→ ${object.local} (${object.size} B) ⇒ cos://${bucket}/${object.key}`);
}

if (dryRun) {
  console.log("dry-run：未上传。");
  process.exit(0);
}

// 必须 trim：在 GitHub 网页上粘贴 secret 很容易带进一个尾随换行，而 COS SDK 会把
// SecretId 原样拼进 `Authorization` 头，一个 `\n` 就让请求报
// `ERR_INVALID_CHAR / Invalid character in header content`——错误信息完全不提换行，
// 看起来像密钥填错了。第一次真发版就栽在这上面。
const secretId = process.env.TENCENT_SECRET_ID?.trim();
const secretKey = process.env.TENCENT_SECRET_KEY?.trim();
if (!secretId || !secretKey) {
  throw new Error("缺少 TENCENT_SECRET_ID / TENCENT_SECRET_KEY");
}
// 密钥中间出现空白说明复制错了（换行被粘到了中间），继续下去只会得到签名错误。
if (/\s/.test(secretId) || /\s/.test(secretKey)) {
  throw new Error("TENCENT_SECRET_ID / TENCENT_SECRET_KEY 中间含有空白字符，请重新设置 secret");
}

const cos = new COS({ SecretId: secretId, SecretKey: secretKey });

for (const object of objects) {
  const name = basename(object.key);
  // 逐个串行上传：并发在 CI 上收益不大，而失败时定位会难很多。
  try {
    const result = await cos.putObject({
      Bucket: bucket,
      Region: region,
      Key: object.key,
      Body: createReadStream(object.local),
      ContentLength: object.size,
      ContentType: contentType(name),
      CacheControl: cacheControl(name),
      // 桶默认可能是私有读；更新清单要能被任何人取到，所以显式给公共读。
      ACL: "public-read",
    });
    console.log(`✓ ${object.key} (ETag ${result.ETag})`);
  } catch (error) {
    // SDK 的错误对象很大（含整个响应头），直接抛出去会把 CI 日志刷满，
    // 而真正有用的只有 code / message / requestId。
    const parts = [
      error?.statusCode && `HTTP ${error.statusCode}`,
      error?.code,
      error?.message,
      error?.RequestId && `requestId=${error.RequestId}`,
    ].filter(Boolean);
    console.error(`✗ ${object.key} 上传失败：${parts.join(" / ") || String(error)}`);
    process.exit(1);
  }
}

console.log("全部上传完成。");
