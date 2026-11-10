# 发版与在线更新

**中文** | [English](RELEASE.en.md)

Qingci Craft Launcher 的更新走「GitHub Actions 构建 → GitHub Release 分发」：CI 打出三平台的
安装包并用私钥签名，连同更新清单 `latest.json` 一起发布到 GitHub Release；客户端启动后读清单，
发现新版本就下载安装包，**先用内置公钥校验签名，校验通过才安装**。

```
tag v0.1.14           ← 以 0.1.14 为例
   │
   ▼
GitHub Actions
   ├─ prepare   校验三处版本号 + 签名密钥（不一致就不让两台机器白编译）
   ├─ build     matrix：Windows NSIS / Linux AppImage + deb / macOS dmg + .app.tar.gz（都签名）
   └─ publish   合并产物 → 生成 latest.json → 建 GitHub Release → 推 COS
                   │
                   └─ GitHub Release（全部产物）
                        ├─ QingciCraftLauncher_0.1.14_x64-setup.exe + .sig
                        ├─ ..._x86_64.AppImage + .sig
                        ├─ ..._universal.app.tar.gz + .sig
                        ├─ ..._universal.dmg / ..._amd64.deb / 便携版
                        └─ latest.json
   │
   ▼
客户端启动 → GET github.com/.../releases/latest/download/latest.json
   ├─ 版本不新 → 什么都不做（不打扰）
   └─ 有新版本 → 「关于」页提示 → 用户点「下载并安装」→ 校验签名 → 静默安装 → 自动重启
```

清单覆盖**三个平台四条目标**：`windows-x86_64`（NSIS）、`linux-x86_64`（AppImage）、
`darwin-aarch64` 与 `darwin-x86_64`（同一个 universal `.app.tar.gz`）。deb 由包管理器管，
便携版不参与自更新，所以没有它们的条目。

## 两条通道与一段过渡期

主通道是 GitHub Release；**COS 没有退役**，仍在发 `latest.json` 与 Windows 安装包。
原因：0.1.14 及更早的客户端端点列表里**只有 COS**，它们永远不会去读 GitHub——
不保留这条通道，老用户就永远收不到更新。等观察一段时间（旧版本占比足够低）再退役 COS，
到时只需要删掉工作流里的推送步骤与 `tauri.conf.json` 里的兜底端点。

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
| `QCL_MS_CLIENT_ID` | Azure 应用注册里的「应用程序(客户端) ID」 |

`QCL_MS_CLIENT_ID` 由 `src-tauri/build.rs` 在**编译期**注入（`cargo:rustc-env`），
所以仓库里搜不到它。缺了它构建不会失败，但产物里的微软登录会静默不可用——
因此发版流程把它做成前置检查，缺了直接失败。

**它不是机密，别把它当机密**。OAuth 2.0 的公共客户端（设备码流程）没有 client secret，
ID 在每次请求的表单里明文出现。编译期注入只解决「仓库里不出现发布者的应用注册」，
**二进制里一定有**——`grep -a <id> 程序` 一秒就能找到。想真正防止别人复用这个 ID，
只能在 Azure 侧处理，代码层面做不到。

贡献者本地没有这个 ID 也能编译、能跑其余功能，只是登录不可用（界面会说明）。
想自己测登录，就注册一个 Azure 应用然后 `QCL_MS_CLIENT_ID=xxx pnpm tauri dev`。

私钥文件是 base64 编码的 minisign 密钥，**只有一行**（`untrusted comment:` 那些
标记被编码在 base64 内部，文件里看不到）。整段复制即可，不要自己加换行。

生成密钥时没有设密码，所以不需要 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。
如果以后带密码重新生成，再加这个 secret。

> 为什么密码可有可无：私钥和密码都放在同一个 secret 存储里，多一个密码并不增加
> 实际防护。只有在把私钥文件放到别处（比如离线保存）时，密码才有意义。

### 3. COS 侧

- 桶：`qcl-1331847184`，地域 `ap-guangzhou`，访问权限**私有读写**，路径前缀 `releases/`
- 自定义域名：`qcl.luoqingciya.cn`，**必须启用 HTTPS**——客户端拒绝 http 端点
- 桶不能开启「防盗链」白名单，否则客户端拿不到清单

**读权限怎么给**。桶是**私有读写**，`releases/` 能被匿名读靠两层同时成立：

| 机制 | 现状 | 依据 |
| --- | --- | --- |
| 对象 ACL | 上传时显式设 `public-read` | `scripts/publish-cos.mjs` 里的 `ACL: "public-read"` |
| 桶策略 | 允许匿名读 `releases/*` 前缀，**不勾「列出对象列表」** | `releases/nope.txt` 返回 404 而非 403 |

两层冗余是有意的，但排查 403 时要知道**有两处**——只改一处可能看不出任何变化。

不要把桶直接设成「公有读私有写」：那会让桶里**任何**东西都可匿名读，而实际只有
`releases/` 需要公开。

两个前提：

1. **桶不能开「禁止对象级 ACL」**。开了之后 `putObject` 带 ACL 会直接报错，上传全挂；
   那种情况下只能靠桶策略，把对象 ACL 那一层去掉。
2. 桶策略的资源写 `qcl-1331847184/releases/*`（**注意是复数 `releases`**）。
   前缀写错一个字母就一条都匹配不上，而且**不会立刻暴露**——对象 ACL 还在，文件照样
   能读，直到 ACL 那层失效才会以 403 的形式出现。

任何时候都可以用这四条命令核对权限是否真的如上面所述（2026-10-27 实测通过）：

```bash
# 1. 清单本身：200 才算通
curl -o /dev/null -w "%{http_code}\n" https://qcl.luoqingciya.cn/releases/latest.json
# 2. 前缀内的不存在的键：404 = 这个前缀允许匿名读
curl -o /dev/null -w "%{http_code}\n" https://qcl.luoqingciya.cn/releases/nope.txt
# 3. 前缀外的不存在的键：403 = 桶确实是私有的
curl -o /dev/null -w "%{http_code}\n" https://qcl.luoqingciya.cn/nope.txt
# 4. 匿名列举：403 才算没开放（否则任何人都能枚举发布历史）
curl -s -o /dev/null -w "%{http_code}\n" "https://qcl.luoqingciya.cn/?prefix=releases/"
```

关键判读：**私有桶对不存在的对象返回 403，允许匿名读的前缀下返回 404**。
只测第 1 条分不清「权限没配好」和「文件还没上传」；几条一起测才能定位。

第 4 条容易被忽略：`/releases/`（带尾斜杠）返回 200 是 COS 的目录占位，**不代表**能列目录；
真正的列举请求带 `?prefix=` 查询串，它返回 403 才说明列表权限没开。

## 发版

```bash
# 1. 版本号要改三处（CI 会校验，不一致直接失败）
#    package.json               "version"
#    src-tauri/tauri.conf.json  "version"      ← 客户端与清单比对的就是它
#    Cargo.toml                 [workspace.package] version
#      └─ 它会经 qcl_core::LAUNCHER_VERSION 进到游戏的 --version 与 HTTP User-Agent

# 2. 重新生成更新日志（同一个 PR 里带上，不要等发完再补）
#    --tag 把「未发布」段直接标成这次要发的版本。不加它，因为标签还没建，
#    git-cliff 只能写「未发布」，发完再补就又多一个 PR。
git cliff --config cliff.toml --tag v0.1.4 -o CHANGELOG.md

# 3. 提交并打标签（下面以 0.1.4 为例，换成实际要发的版本）
git commit -am "chore(release): 0.1.4"
git tag v0.1.4
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

### 发版前冒烟（安装链路）

界面点不出「空实例」这种场景，而历史上几个阻断性 bug 全是在空目录上才复现的
（缺原版 jar、加载器种类不同步、切种类点到旧列表）。用 CLI 跑一遍：

```bash
# 空实例 + Forge：覆盖「先装原版 → 再装加载器」这条最容易写错的顺序
cargo run -p qcl-cli -- 1.20.1 --dir /tmp/qcl-smoke-forge --fresh \
  --loader forge:47.2.0 --expect "Forge mod loading"

# 空实例 + Fabric
cargo run -p qcl-cli -- 1.20.1 --dir /tmp/qcl-smoke-fabric --fresh \
  --loader fabric:0.15.0 --expect "Setting user"
```

- `--fresh` 要求目录为空（不存在或空目录），**不会替你清空**——写错路径时拒绝执行。
- `--expect <文本>` 启动后等日志出现该文本，命中即停游戏并返回 0；超时返回非 0。
- 没有显示器或不想真启动时用 `--dry-run`：装完只打印启动命令。
- 顺序与桌面端一致（原版 → 加载器 → 合并版本补文件），所以能挡住同一类错误。

## 平台现状

主通道是 GitHub Release，COS 是过渡期的清单兜底：

| 通道 | 内容 | 用途 |
| --- | --- | --- |
| GitHub Releases（主） | 三平台安装包 + `.sig` + `latest.json`、便携版 | **自动更新通道**，也是人工下载页 |
| COS（`qcl.luoqingciya.cn/releases/`） | Windows NSIS 安装包 + `.sig` + `latest.json` | **0.1.14 及更早客户端的清单兜底**，迁移完退役 |

`latest.json` 覆盖四条目标：`windows-x86_64`、`linux-x86_64`（AppImage）、
`darwin-aarch64` 与 `darwin-x86_64`（同一个 universal `.app.tar.gz`）。全部下载地址
都指向 GitHub Release——包括 COS 上那份清单里的，这样旧客户端迁移到 GitHub 后，
新版本也不再依赖 COS。

体积取自 0.1.5 的实测产物，版本之间会有小幅浮动（前端改动大时更明显）——发布前以实际产物为准，
不要照抄这几个数字。

- **Windows 安装版**：NSIS 3.8 MB，复用系统 WebView2，自动更新走静默安装 + 重启。
- **Windows 便携版**：12.7 MB，就是安装包里那个 exe，双击即用，游戏目录落在 exe 旁边。
  会收到更新提示，安装的是 NSIS 版（等于升级成安装版）；便携版目录本身不会被替换。
- **macOS 通用 dmg + `.app.tar.gz`**：12.1 MB，`--target universal-apple-darwin`，
  一次构建同时含 arm64 与 x86_64，用户不必挑架构。**未签名、未公证**：没有付费开发者账号，
  用户首次打开要右键「打开」或 `xattr -dr com.apple.quarantine`，README 里写了三种方式。
  dmg 用于首次安装；自动更新走 `.app.tar.gz`——它的解压走 tar+gz，**不依赖**
  因 RUSTSEC-2026-0367/0368 关掉的 `zip` 特性（曾误以为依赖，查过
  tauri-plugin-updater 2.14.0 的源码后更正，见 `src-tauri/Cargo.toml`）。
- **Linux AppImage**：83.5 MB。自包含，把 WebKitGTK + JavaScriptCore + GTK3 整套塞进去了
  （解出来 530 个文件），所以体积是 deb 的十几倍。自动更新直接替换 AppImage 文件。
- **Linux deb**：5.3 MB，依赖发行版的 `libwebkit2gtk-4.1`。**没有更新条目**：
  deb 的文件由包管理器管，updater 替换 `/usr/bin` 下的文件既越权也会和 dpkg 打架，
  所以 deb 用户保持手动更新。
- macOS 的产物仍未做 Apple 签名/公证（无付费开发者账号）；minisign 签名照常生成，
  更新安装不受 Gatekeeper 影响（quarantine 属性只出现在浏览器下载的文件上）。

## 客户端行为

- 启动后静默查一次；查到新版本才在「关于」标签上点红点，查不到或网络失败**什么都不显示**
  （一个「检查更新失败」的红条出现在刚打开的启动器上，只会让人以为程序坏了）
- 关于页可以手动「检查更新」，失败时会显示原因
- 端点按顺序尝试：GitHub 在前，COS 兜底——GitHub 的 `releases/latest/download/`
  是 302 跳转，偶发失败时会落到 COS
- 安装包签名对不上时**拒绝安装**（`requireSignedVersion` 还会拒绝「清单里的版本号与
  签名里的版本号不一致」的降级攻击）

## 排错

| 现象 | 查什么 |
| --- | --- |
| 关于页一直是「已是最新版本」 | `curl -L https://github.com/luoqingciya/Qingci-Craft-Launcher/releases/latest/download/latest.json` 是否 200、版本号是否比本地大 |
| 提示检查失败 | 两个端点（GitHub、COS）是否都可达；`latest.json` 是否真的在该 Release 的资产里 |
| 下载到 100% 后失败 | 签名不匹配：私钥与 `pubkey` 不是一对，或清单里的 `signature` 不是 `.sig` 原文 |
| 安装后版本没变 | 装到了另一个目录（便携版被当成了安装版），或同时存在 MSI 装的旧版本 |
| Linux 上更新失败 | 只支持 AppImage 方式运行；deb 安装的请走包管理器 |
| 切到 BMCLAPI 后没变快，甚至仍报网络错误 | 镜像失败会**自动回落官方**。BMCLAPI 的元数据端点会 302 到它自己的 CDN（`bmclapi*.sakura.sld.tw`），部分网络不通；Forge maven 那条线则走 CERNET/USTC，通常可用 |
