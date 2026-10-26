# 贡献指南

**中文** | [English](CONTRIBUTING.en.md)

本项目采用**严格的 PR 流程**：`main` 受保护，任何改动都必须通过分支 + Pull Request 合入。

## 1. 环境准备

| 依赖 | 版本 | 说明 |
|---|---|---|
| Rust | 1.99.1 | 由 `rust-toolchain.toml` 固定，`rustup` 首次执行 `cargo` 时自动安装 |
| Node.js | ≥ 22 | 前端与 Tauri CLI |
| pnpm | ≥ 10 | 包管理器，禁止混用 npm / yarn 产生其他锁文件 |

Windows 需要 MSVC Build Tools；Linux 需要 `libwebkit2gtk-4.1-dev`、`libappindicator3-dev`、`librsvg2-dev`、`patchelf`。

## 2. 提交前自检

提交 PR 前，本地必须通过以下命令（与 CI 完全一致）：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-features
cargo deny check licenses bans sources

pnpm lint
pnpm test
pnpm build
```

`cargo deny` 的 `advisories` 需要拉取 RustSec 库，网络受限时可以只跑
`licenses bans sources` 三项——CI 上会跑全量。

## 3. 分支命名

| 前缀 | 用途 | 示例 |
|---|---|---|
| `feat/` | 新功能 | `feat/core-rules-eval` |
| `fix/` | 缺陷修复 | `fix/java-path-with-spaces` |
| `refactor/` | 不改变行为的重构 | `refactor/downloader-split` |
| `docs/` | 文档 | `docs/development-plan` |
| `ci/` | CI / 构建 | `ci/pr-gates` |
| `chore/` | 依赖、配置、脚手架 | `chore/repo-bootstrap` |

分支生命周期目标 ≤ 3 天。大特性请拆成**堆叠 PR（stacked PR）**，在描述中注明依赖顺序与目标分支。

## 4. 提交信息规范

遵循 [Conventional Commits](https://www.conventionalcommits.org/)：

```
<type>(<scope>): <subject>
```

允许的 `type`：`feat` `fix` `docs` `style` `refactor` `perf` `test` `build` `ci` `chore` `revert`
常用 `scope`：`core` `net` `auth` `java` `app` `ui` `loader` `mods` `update` `release` `ci` `deps`

PR 标题必须符合该格式，因为合并时使用 **Squash merge**，PR 标题即最终的提交信息。

## 5. PR 硬性规则

1. **一 PR 一事**。功能变更与重构不得混在同一个 PR。
2. **有效 diff ≤ 400 行**（不含测试、生成代码、锁文件）。超限须拆分。
   仓库基建类 PR（模板、配置文件、文档）不受此限，但须在描述中说明构成。
3. **必须含测试**：新逻辑要有单测；修 bug 要有能复现该 bug 的回归测试。
4. **必须更新文档**：涉及架构、IPC 契约或配置格式的变更，同步更新 `docs/`；重大技术选型补一篇 ADR（`docs/adr/`）。
5. **禁止提交**：注释掉的代码、`dbg!` / `println!` 调试输出、密钥与本地绝对路径、未使用的依赖。
6. **CI 全绿 + 完成 PR 模板自检清单**后才可请求评审。

## 6. 评审与合并

- 至少 1 个 approval，且必须解决全部评审意见。
- 分支必须与 `main` 同步（`Require branches to be up to date`）。
- 使用 **Squash and merge**，合并后自动删除分支。
- 禁止 force push 到受保护分支。

## 7. 代码风格

- Rust 格式由 `rustfmt.toml` 决定，不接受格式化争论。
- `clippy -D warnings` 视为错误；确有必要的例外需在代码处 `#[allow(...)]` 并写明理由。
- 错误类型：库层用 `thiserror` 定义可分类错误，应用层用 `anyhow` 附加上下文。
- 公共 API 必须有文档注释（`missing_docs` 为 warning，CI 会将其提升为错误）。
- 前端 lint 用 **oxlint**（`pnpm lint`），配置在 `.oxlintrc.json`。不要引入 eslint：
  `typescript-eslint` 的 peer 依赖不支持本项目使用的 TS 7。
- 纯逻辑放 `src/lib/`，组件只做渲染：纯函数测试毫秒级且不依赖 DOM。
- 测试里若涉及平台差异（路径形态、shell 语义、`#[cfg]` 导入），先自问
  「另一个平台这一行还成立吗」——本机通过不代表 CI 通过。

### 调界面时用预览页

`LauncherView` 是纯展示组件，`pnpm dev` 后打开
<http://localhost:5173/preview.html> 就能用假数据渲染完整界面。
调布局与配色不必启动桌面壳，也不必真的下载一个版本。

改动界面时请先在这个页面确认，再更新 `src/preview.tsx` 里的假数据。

## 8. 更新日志

`CHANGELOG.md` 由 [git-cliff](https://github.com/orhun/git-cliff) 依据 Conventional Commits 自动生成，
**不手工编辑**。

```bash
git cliff --config cliff.toml -o CHANGELOG.md    # 重新生成全量
git cliff --config cliff.toml --unreleased       # 仅预览未发布部分
```

生成时机是**发布前**，而不是每个 PR。原因：未发布段落由「上一个 tag 之后的全部提交」构成，
若要求每个 PR 都更新它，多个并行 PR 会持续产生冲突，且每次合并都要重排。

**具体落在哪一步**：发版 PR（`chore(release): x.y.z`）必须包含重新生成的 `CHANGELOG.md`。
这条是补写的——v0.1.1 与 v0.1.2 两次发版都忘了生成，日志停在项目最初 5 个 PR 上，
直到 v0.1.2 之后才补回来。规则不落到某一步骤上，就等于没有规则。

`git-cliff` 不必装进环境：可以从 [releases](https://github.com/orhun/git-cliff/releases)
下预编译包，或 `cargo install git-cliff`。

由此得到一个推论：**PR 标题会原样出现在更新日志里**。这就是第 4 节要求标题严格遵循
Conventional Commits 的实际原因——规范与否直接决定用户看到的是「feat(core): 版本清单与版本元数据的领域模型」
还是「update stuff」。


## 9. 许可与版权

本项目以 **GPL-3.0** 发布，并附有 **GPLv3 第 7 条附加条款**，见
[ADDITIONAL-TERMS.md](ADDITIONAL-TERMS.md)。两条要求与你直接相关：

1. **提交贡献即视为同意**：你的贡献以 GPL-3.0 授权，并受该附加条款约束。若不接受，请勿提交。
2. **新文件要带文件头**：所有 `.rs` / `.ts` / `.tsx` 文件开头须有三行声明（照抄即可）：

   ```
   Copyright (C) 2026 luoqingci
   SPDX-License-Identifier: GPL-3.0-only
   附加条款（第 7 条）：见 ADDITIONAL-TERMS.md
   ```

   这不是形式主义：GPLv3 第 7 条要求附加条款出现在相关源文件里，或至少给出「到哪里找」的提示。
   漏掉文件头的 PR 会被要求补上。

另外，**分发修改版必须更换名称**（不得继续使用「Qingci Craft Launcher」「QCL」等），
但原样分发未经修改的副本不受此限制。

## 10. 模组中文译名库

Modrinth 的搜索索引只有英文（中文关键词返回 0 条），所以中文名靠一份本地译名库：
`assets/data/mod-zh.json`，键是项目的 **slug**，值是中文名。

```json
{
  "sodium": "钠",
  "create": "机械动力"
}
```

补一条译名的流程：

1. 在 `https://modrinth.com/mod/<slug>` 确认 slug 与项目对得上
2. 往 JSON 里加一行（**一行一条**，便于 review 与合并）
3. `cargo test -p qcl-mods` —— 会校验 JSON 合法、无空值、**无重复键**

几条约定：

- 只译**名字**，不译描述。描述是长文本、术语密集，机翻质量不稳；界面上给了
  「在 Modrinth 查看」入口。
- 专有名词可以保留原文（如 `Fabric API`、`Jade`）。
- 名字里带说明时用中点分隔：`"iris": "铱 · 光影加载器"`。
- 译名库随安装包发布，不联网更新——所以合并后要等下一个版本才生效。

同目录下还有两张表，维护方式相同：

| 文件 | 作用 |
|---|---|
| `mod-zh.json` | slug → 中文名（结果显示用） |
| `mod-search-aliases.json` | 中文搜索词 → 英文关键词（输入「钠」时实际搜 `sodium`） |
| `mod-categories-zh.json` | Modrinth 分类 id → 中文标签（筛选按钮用） |

分类固定 19 个（Modrinth 的标签集），不要增删 id；只调整中文标签。
别名表加一行即可生效——输入中文时后端先查这张表，查不到就原样搜。

## 11. 发版

发版是维护者的事，但改动可能影响发版链路，所以规则写在这里：

- **版本号有三处**：`package.json`、`src-tauri/tauri.conf.json`、`Cargo.toml` 的
  `[workspace.package]`。三处必须一致，发版工作流会校验，不一致直接失败。
- **不要在 PR 里改版本号**，除非那个 PR 本身就是发版准备。日常 PR 改版本号会让并行的
  分支互相冲突。
- 改了 `src-tauri/tauri.conf.json`、`src-tauri/capabilities/`、`.github/workflows/release.yml`
  或 `scripts/` 时，请先读 [docs/RELEASE.md](docs/RELEASE.md)——这些文件出错只会在发版时才暴露。

完整的发版流程、签名密钥管理与排错见 [docs/RELEASE.md](docs/RELEASE.md)（[English](docs/RELEASE.en.md)）。
