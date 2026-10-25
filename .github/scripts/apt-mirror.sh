#!/usr/bin/env bash
# Copyright (C) 2026 luoqingci
# SPDX-License-Identifier: GPL-3.0-only
# 附加条款（第 7 条）：见 ADDITIONAL-TERMS.md
#
# 给 CI 选一个快的 Ubuntu 镜像源。
#
# 为什么要探测而不是写死：GitHub 托管的 runner 位置不固定，写死一个「国内快」的
# 源在美国 runner 上反而更慢。实测数据说明这件事很重要——apt 装 webkit 依赖
# 曾占掉 build job 305 秒里的 166 秒。
#
# 探测方式：对每个候选源取 Release 文件，比较耗时，选最快的。全部失败时保持原样，
# 不让镜像问题把 CI 直接搞挂。
#
# 一个必须注意的细节：**不替换 security.ubuntu.com**。它只托管 *-security 套件，
# 不托管 noble / noble-updates。把整份 sources 都指向归档镜像会让 apt-get update
# 因为找不到套件而失败——那种失败看起来像网络问题，实际是配置错误。

set -euo pipefail

# 非 Ubuntu 环境（例如本地 macOS 上手动跑）也要能正常退出，而不是在这里报错。
if [[ -r /etc/os-release ]]; then
  # shellcheck disable=SC1091
  . /etc/os-release
fi
CODENAME="${VERSION_CODENAME:-noble}"

CANDIDATES=(
  "http://azure.archive.ubuntu.com/ubuntu"
  "http://archive.ubuntu.com/ubuntu"
  "https://mirrors.edge.kernel.org/ubuntu"
  "https://mirrors.tuna.tsinghua.edu.cn/ubuntu"
  "https://mirrors.aliyun.com/ubuntu"
)

best_mirror=""
best_time=""

echo "探测 apt 镜像（发行版代号：${CODENAME}）"

# 探一个源，输出「状态码 耗时」。
probe() {
  curl -s -o /dev/null -m 8 \
    -w '%{http_code} %{time_total}' \
    "$1/dists/${CODENAME}/Release" 2>/dev/null || echo "000 99"
}

# 快路径：默认源（Azure）在 GitHub runner 上通常 0.01 秒就响应，此时再探其它源
# 纯属浪费——实测探测 5 个源要 5 秒，而整个安装步骤才 60 秒。
first="${CANDIDATES[0]}"
measured=$(probe "$first")
code="${measured%% *}"
elapsed="${measured##* }"

if [[ "$code" == "200" ]] && awk -v t="$elapsed" 'BEGIN { exit !(t < 0.5) }'; then
  printf '  %-52s %6.2fs  ok（默认源已足够快，跳过其它候选）\n' "$first" "$elapsed"
  best_mirror="$first"
  best_time="$elapsed"
fi

best_mirror="${best_mirror:-}"
best_time="${best_time:-}"

if [[ -z "$best_mirror" ]]; then
  for mirror in "${CANDIDATES[@]}"; do
    # 用命令替换而不是 `read < <(...)`：`curl -w` 不输出换行，`read` 会返回 1，
    # 配上 `set -e` 会让脚本在第一次探测就退出（CI 上真的踩到了）。
    measured=$(probe "$mirror")
    # 状态码与耗时一起取：只看耗时会把「秒回 404」当成最快的源。
    code="${measured%% *}"
    elapsed="${measured##* }"

    if [[ "$code" == "200" ]]; then
      printf '  %-52s %6.2fs  ok\n' "$mirror" "$elapsed"
      # 用 awk 比较浮点，不依赖 bc（runner 上不保证装了）。
      if [[ -z "$best_time" ]] || awk -v a="$elapsed" -v b="$best_time" 'BEGIN { exit !(a < b) }'; then
        best_time="$elapsed"
        best_mirror="$mirror"
      fi
    else
      printf '  %-52s %6.2fs  HTTP %s\n' "$mirror" "$elapsed" "${code:-?}"
    fi
  done
fi

if [[ -z "$best_mirror" ]]; then
  echo "所有候选源都不可用，保持默认配置"
  exit 0
fi

printf '选用：%s（%.2fs）\n' "$best_mirror" "$best_time"

# 新版 Ubuntu 用 deb822 格式（sources.list.d/ubuntu.sources），旧版用 sources.list。
# 两种都要处理，否则镜像版本升级后这里会静默失效。
if [[ -f /etc/apt/sources.list.d/ubuntu.sources ]]; then
  # 跳过 URIs 行里含 security.ubuntu.com 的段落。
  sudo sed -i -E \
    "/^URIs: .*security\.ubuntu\.com/! s|^URIs: .*|URIs: ${best_mirror}|" \
    /etc/apt/sources.list.d/ubuntu.sources
fi
if [[ -f /etc/apt/sources.list ]]; then
  # 只替换归档域名，security.ubuntu.com 原样保留。
  sudo sed -i -E \
    "s|https?://(azure\.)?archive\.ubuntu\.com/ubuntu|${best_mirror}|g" \
    /etc/apt/sources.list
fi

echo "当前 apt 源："
grep -hE '^(URIs:|deb )' /etc/apt/sources.list /etc/apt/sources.list.d/*.sources 2>/dev/null | head -8 || true
