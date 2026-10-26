# 发版与在线更新

[English](RELEASE.en.md) | 中文

清辞的更新走「GitHub Actions 构建 → 腾讯云 COS 分发」：CI 打出 NSIS 安装包并用
私钥签名，连同更新清单 `latest.json` 一起推到 COS；客户端启动后读清单，发现新版本
就下载安装包，**先用内置公钥校验签名，校验通过才安装**。

```
tag v0.1.1
   │
   ▼
GitHub Actions (windows-latest)
   ├─ pnpm tauri build --bundles nsis
   ├─ 产物：QingciCraftLauncher_0.1.1_x64-setup.exe + .sig
   ├─ scripts/make-latest-json.mjs → latest.json
   └─ scripts/publish-cos.mjs → cos://qcl-1331847184/releases/
                                   ├─ QingciCraftLauncher_0.1.1_x64-setup.exe
                                   ├─ ...exe.sig
                                   └─ latest.json
   │
   ▼
客户端启动 → GET https://qcl.luoqingciya.cn/releases/latest.json
   ├─ 版本不新 → 什么都不做（不打扰）
   └─ 有新版本 → 「关于」页提示 → 用户点「下载并安装」→ 校验签名 → 静默安装 → 自动重启
```

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
| `TAURI_SIGNING_PRIVATE_KEY` | **私钥文件的完整内容**（含两行注释） |

生成密钥时没有设密码，所以不需要 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。
如果以后带密码重新生成，再加这个 secret。

> 为什么密码可有可无：私钥和密码都放在同一个 secret 存储里，多一个密码并不增加
> 实际防护。只有在把私钥文件放到别处（比如离线保存）时，密码才有意义。

### 3. COS 侧

- 桶：`qcl-1331847184`，地域 `ap-guangzhou`，路径前缀 `releases/`
- 自定义域名：`qcl.luoqingciya.cn`，**必须启用 HTTPS**——客户端拒绝 http 端点
- 对象权限：上传脚本对每个对象显式设 `public-read`。
  如果桶开启了「禁止对象级 ACL」，需要改为给桶开公有读，或给这个前缀单独授权匿名读
- 桶不能开启「防盗链」白名单，否则客户端拿不到清单

## 发版

```bash
# 1. 版本号要改两处（CI 会校验，不一致直接失败）
#    package.json        "version"
#    src-tauri/tauri.conf.json  "version"

# 2. 提交并打标签
git commit -am "chore: 发布 0.1.1"
git tag v0.1.1
git push origin main --tags
```

标签推上去后 `Release` workflow 自动跑：构建 → 签名 → 生成清单 → 推 COS → 建
GitHub Release（附安装包与 `.sig`）。

**不要复用已存在的标签**：客户端按 `latest.json` 里的版本号判断新旧，同一个版本号
重新发布，已经升级过的用户不会再更新一次。要重发就换个更大的版本号。

### 先验证再发布

`workflow_dispatch` 默认 `dry_run = true`：只构建并打印 `latest.json` 与将要上传的
对象，不碰 COS、不建 Release。第一次配好之后建议先这样跑一遍。

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
