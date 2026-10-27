# 发版与在线更新

[English](RELEASE.en.md) | 中文

Qingci Craft Launcher 的更新走「GitHub Actions 构建 → 腾讯云 COS 分发」：CI 打出 NSIS 安装包并用
私钥签名，连同更新清单 `latest.json` 一起推到 COS；客户端启动后读清单，发现新版本
就下载安装包，**先用内置公钥校验签名，校验通过才安装**。

```
tag v0.1.2
   │
   ▼
GitHub Actions
   ├─ prepare   校验三处版本号 + 签名密钥（不一致就不让两台机器白编译）
   ├─ build     matrix：Windows 出 NSIS / Linux 出 AppImage + deb（都签名）
   └─ publish   合并产物 → 生成 latest.json → 推 COS → 建 GitHub Release
                   │
                   ├─ COS（只放自动更新需要的）
                   │    ├─ QingciCraftLauncher_0.1.2_x64-setup.exe
                   │    ├─ ...exe.sig
                   │    └─ latest.json            ← 最后传
                   │
                   └─ GitHub Release（全部产物）
                        └─ 上面三个 + AppImage + AppImage.sig + deb
   │
   ▼
客户端启动 → GET https://qcl.luoqingciya.cn/releases/latest.json
   ├─ 版本不新 → 什么都不做（不打扰）
   └─ 有新版本 → 「关于」页提示 → 用户点「下载并安装」→ 校验签名 → 静默安装 → 自动重启
```

清单里只有 `windows-x86_64` 一条：Linux 的 AppImage 有 83 MB，又不参与自动更新，
放 COS 上只是白占带宽，所以它和 deb 一起只留在 GitHub Release。

## 一次性配置

### 1. 签名密钥

更新包必须签名，客户端内置公钥。**私钥丢了就再也发不出能被现有客户端接受的更新**，
只能让所有用户手动重装。

密钥对已生成在本机：

| 文件 | 用途 |
| --- | --- |
| `%USERPROFILE%\.tauri\qcl-updater.key` | 私钥，贴进 GitHub secret，**不要进仓库** |
| `%USERPROFILE%\.tauri\qcl-updater.key.pub` | 公钥，已写进 `src-tauri/tauri.conf.json` 的 `plugins.updater.pubkey` |

签名只在发版时发生：`createUpdaterArtifacts` 写在 `src-tauri/tauri.release.json` 里，
由发版工作流用 `--config` 合并进来。贡献者本地 `pnpm tauri build` 不需要任何密钥。

需要重新生成时（会作废所有已发布客户端）：

```bash
pnpm tauri signer generate -w ~/.tauri/qcl-updater.key --ci
```

### 2. GitHub 仓库配置

Settings → Secrets and variables → Actions：

| 名称 | 值 |
| --- | --- |
| `TENCENT_SECRET_ID` | 腾讯云 API 密钥 |
| `TENCENT_SECRET_KEY` | 同上 |
| `TAURI_SIGNING_PRIVATE_KEY` | **私钥文件的全部内容**：一整行 base64，没有换行 |

私钥文件是 base64 编码的 minisign 密钥，**只有一行**（`untrusted comment:` 那些
标记被编码在 base64 内部，文件里看不到）。整段复制即可，不要自己加换行。

生成密钥时没有设密码，所以不需要 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。
如果以后带密码重新生成，再加这个 secret。

> 为什么密码可有可无：私钥和密码都放在同一个 secret 存储里，多一个密码并不增加
> 实际防护。只有在把私钥文件放到别处（比如离线保存）时，密码才有意义。

### 3. COS 侧

- 桶：`qcl-1331847184`，地域 `ap-guangzhou`，路径前缀 `releases/`
- 自定义域名：`qcl.luoqingciya.cn`，**必须启用 HTTPS**——客户端拒绝 http 端点
- 桶不能开启「防盗链」白名单，否则客户端拿不到清单

**读权限怎么给**。桶的读写权限与对象的读权限是两件事：

| 方案 | 桶权限 | 靠什么让 `releases/` 可匿名读 |
| --- | --- | --- |
| A（当前） | 公有读私有写 | 桶权限本身就放开了读 |
| B（更收敛，推荐） | **私有读写** | 上传脚本给每个对象显式设 `public-read` |

从 A 换到 B 是安全的，因为 `scripts/publish-cos.mjs` 上传时带了 `ACL: public-read`。
两点要注意：

1. **换之前上传的对象可能跟着桶一起变私有**。换完之后立刻验证：
   `curl -I https://qcl.luoqingciya.cn/releases/latest.json` 必须是 200；
   若变 403，在控制台把那两个对象设为「公有读私有写」即可。
2. 如果桶开了「禁止对象级 ACL」，方案 B 就不成立——putObject 会报错。此时改用桶策略：
   资源写 `qcl-1331847184/releases/*`（**注意是复数 `releases`**），授权操作选「读操作」，
   不要勾「列出对象列表」。策略里的前缀写错一个字母就一条都匹配不上，而桶是公有读时
   这个错误完全不会暴露——等切到私有读写那天才会以 403 的形式出现。

改完权限用这三条命令验证，它们能区分「桶是否真的私有了」和「前缀是否真的生效」：

```bash
# 1. 清单本身：200 才算通
curl -o /dev/null -w "%{http_code}\n" https://qcl.luoqingciya.cn/releases/latest.json
# 2. 前缀内的不存在的键：404 = 这个前缀允许匿名读
curl -o /dev/null -w "%{http_code}\n" https://qcl.luoqingciya.cn/releases/nope.txt
# 3. 前缀外的不存在的键：403 = 桶确实是私有的
curl -o /dev/null -w "%{http_code}\n" https://qcl.luoqingciya.cn/nope.txt
```

关键判读：**私有桶对不存在的对象返回 403，允许匿名读的前缀下返回 404**。
只测第 1 条分不清「权限没配好」和「文件还没上传」；三条一起测才能定位。

## 发版

```bash
# 1. 版本号要改三处（CI 会校验，不一致直接失败）
#    package.json               "version"
#    src-tauri/tauri.conf.json  "version"      ← 客户端与清单比对的就是它
#    Cargo.toml                 [workspace.package] version
#      └─ 它会经 qcl_core::LAUNCHER_VERSION 进到游戏的 --version 与 HTTP User-Agent

# 2. 重新生成更新日志（同一个 PR 里带上，不要等发完再补）
git cliff --config cliff.toml -o CHANGELOG.md

# 3. 提交并打标签（下面的版本号换成实际要发的）
git commit -am "chore(release): 0.1.3"
git tag v0.1.3
git push origin main --tags
```

第 2 步容易漏——v0.1.1 和 v0.1.2 都漏了，日志停在项目最初 5 个 PR 上，事后才补回来。
原因很朴素：规则只写在 CONTRIBUTING 里说「发布前生成」，没落到具体某一步，就一定会忘。

标签推上去后 `Release` workflow 自动跑：构建 → 签名 → 生成清单 → 推 COS → 建
GitHub Release（附安装包与 `.sig`）。

**不要复用已存在的标签**：客户端按 `latest.json` 里的版本号判断新旧，同一个版本号
重新发布，已经升级过的用户不会再更新一次。要重发就换个更大的版本号。

### 先验证再发布

`workflow_dispatch` 默认 `dry_run = true`：只构建并打印 `latest.json` 与将要上传的
对象，不碰 COS、不建 Release。第一次配好之后建议先这样跑一遍。

## 平台现状

两条分发通道，故意分开：

| 通道 | 内容 | 用途 |
| --- | --- | --- |
| COS（`qcl.luoqingciya.cn/releases/`） | Windows NSIS 安装包 + `.sig` + `latest.json` | **自动更新通道** |
| GitHub Releases | 上面那些，外加 Linux 的 AppImage 与 deb | 人工下载 |

所以 `latest.json` 里**只有** `windows-x86_64` 一条。Linux 产物留在 GitHub Releases：
AppImage 有 83 MB，又不参与自动更新，放 COS 上只是白占带宽。

- **Windows**：NSIS 3.5 MB，复用系统 WebView2，自动更新走静默安装 + 重启。
- **Linux AppImage**：83 MB。自包含，把 WebKitGTK + JavaScriptCore + GTK3 整套塞进去了
  （解出来 530 个文件），所以体积是 deb 的十几倍。
- **Linux deb**：4.9 MB，依赖发行版的 `libwebkit2gtk-4.1`。
- **Linux 整体不提供自动更新**。「关于」页会识别出 Linux，直接引导到下载页，
  而不是给一个点了就报错的按钮。
- **macOS**：不发布。自动更新需要 updater 的 `zip` 特性来解压 `.app.tar.gz`，而该特性
  目前因为两个解压漏洞（RUSTSEC-2026-0367 / 0368）被关掉了，见 `src-tauri/Cargo.toml`；
  另外未签名的 dmg 会被 Gatekeeper 直接拦下，正常分发需要付费开发者账号与公证。

## 客户端行为

- 启动后静默查一次；查到新版本才在「关于」标签上点红点，查不到或网络失败**什么都不显示**
  （一个「检查更新失败」的红条出现在刚打开的启动器上，只会让人以为程序坏了）
- 关于页可以手动「检查更新」，失败时会显示原因
- **只支持安装版自动更新**：便携版没有 `uninstall.exe`，Windows 不允许替换正在运行的
  exe，界面改为引导用户去下载页
- 安装包签名对不上时**拒绝安装**（`requireSignedVersion` 还会拒绝「清单里的版本号与
  签名里的版本号不一致」的降级攻击）

## 排错

| 现象 | 查什么 |
| --- | --- |
| 关于页一直是「已是最新版本」 | `curl https://qcl.luoqingciya.cn/releases/latest.json` 是否 200、版本号是否比本地大 |
| 提示检查失败 | 域名证书、桶权限、`latest.json` 是否真的在 `releases/` 下 |
| 下载到 100% 后失败 | 签名不匹配：私钥与 `pubkey` 不是一对，或清单里的 `signature` 不是 `.sig` 原文 |
| 安装后版本没变 | 装到了另一个目录（便携版被当成了安装版），或同时存在 MSI 装的旧版本 |
