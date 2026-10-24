## 变更说明

<!-- 做了什么、为什么做。对应规划中的哪个 Phase / PR 条目。 -->

## 关联

<!-- 如：Closes #12；docs/DEVELOPMENT_PLAN.md Phase 1 PR #5 -->

## 测试方式

<!-- 新增或修改了哪些测试；手工验证的步骤与结果。 -->

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace --all-features`

## 风险与回滚

<!-- 可能出问题的地方；一旦出问题如何回滚。 -->

## 截图 / 日志

<!-- UI 变更必填，否则删除本节。 -->

## 自检清单

- [ ] 一 PR 一事，未混入无关重构
- [ ] 有效 diff ≤ 400 行（不含测试、生成代码、锁文件），超限已拆分或说明理由
- [ ] 新增逻辑有单测；修 bug 有可复现的回归测试
- [ ] 涉及架构 / IPC 契约 / 配置格式变更时已同步更新 `docs/`
- [ ] 无调试输出、注释掉的代码、密钥、本地绝对路径
- [ ] 提交信息符合 Conventional Commits
