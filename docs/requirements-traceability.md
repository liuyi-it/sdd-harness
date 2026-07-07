# 需求追踪矩阵

状态说明：`已验证` 表示存在实现、行为测试和可重复执行的验证命令；`CI 配置` 表示组合矩阵已经定义，实际运行结果以 GitHub Actions 为准。

| 需求章节   | 验收内容                                            | 实现位置                                                                    | 自动化证据                                                                                          | 状态    |
| ---------- | --------------------------------------------------- | --------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- | ------- |
| 2.4        | Claude/Codex 共享确定性 Core                        | `packages/core/src/core.ts`、`contracts.ts`、两端 Adapter                   | `contracts.test.ts`、`adapter-contract.test.ts`                                                     | 已验证  |
| 3.1        | `.sdd/` 是唯一运行时事实源                          | `commands/*`、`ArtifactWriter`、`StateStore`                                | `artifact-writing-regression.test.ts`、`workflow.test.ts`                                           | 已验证  |
| 3.2、4.1   | 原版 codebase-memory-mcp 与降级扫描                 | `codebase/codebase-adapter.ts`                                              | `codebase-adapter.test.ts`、可选 `codebase-adapter.real.test.ts`                                    | 已验证  |
| 3.3、4.2   | OpenSpec 固定快照与内置 SpecEngine                  | `vendor/openspec/`、`engines/openspec/`、`engines/spec/`                    | `openspec-engine.test.ts`、`spec-engine.test.ts`、`dependency-metadata.test.ts`                     | 已验证  |
| 3.4、4.3   | Superpowers 固定快照与内置 TDD 工作流               | `vendor/superpowers/`、`engines/superpowers/`、`engines/tdd/`               | `superpowers-engine.test.ts`、`design-plan.test.ts`、`dependency-metadata.test.ts`                  | 已验证  |
| 3.5、17.7  | 许可证、版本、commit、逐文件清单                    | `vendor/*/VERSION.json`、`MANIFEST.sha256`、`THIRD_PARTY_NOTICES.md`        | `dependency-metadata.test.ts`、`release-validation.test.ts`                                         | 已验证  |
| 7          | 参数、help/version、输出、错误与退出码              | `adapters/host-adapter.ts`、`contracts.ts`                                  | `contracts.test.ts`、`adapter-contract.test.ts`                                                     | 已验证  |
| 8–12       | 状态机、恢复、幂等与 candidate                      | `state/`、`commands/recovery.ts`、`ArtifactWriter`                          | `state.test.ts`、`artifact-writer.test.ts`、各命令测试                                              | 已验证  |
| 13–14      | 原子 state、迁移、全局锁与并发                      | `state/state-store.ts`、`file-lock.ts`                                      | `state.test.ts`、`acceptance.test.ts`                                                               | 已验证  |
| 15         | 归档后只读与 marker/state 收敛                      | `commands/archive.ts`、`commands/change-id.ts`                              | `quality-commands.test.ts`                                                                          | 已验证  |
| 16         | Git baseline、精确文件范围、命令白名单、隔离工作区  | `git/`、`git-isolation/`、`security/`、`commands/build.ts`                  | `git-scope.test.ts`、`build.test.ts`、`worktree.test.ts`、`scope-overlap.test.ts`                   | 已验证  |
| 17         | 提示注入、MCP 信任边界、脱敏与路径安全              | `codebase/`、`audit/`、`security/`、`commands/review.ts`                    | `security.test.ts`、`secrets-scanner.test.ts`、`prompt-injection-guard.test.ts`、`workflow.test.ts` | 已验证  |
| 18         | 配置、状态、任务、Loop 与运行级结果 Schema 1.2.0    | `schemas/`、`install/canonical-schemas.ts`                                  | `schema-golden.test.ts`、`npm run validate:schemas`                                                 | 已验证  |
| 19         | Context Pack、项目规则哈希与目录规范新鲜度          | `commands/plan.ts`、`commands/build.ts`、`project-conventions/`             | `design-plan.test.ts`、`build.test.ts`、`project-conventions.test.ts`                               | 已验证  |
| 20         | JSONL 审计、轮转与脱敏                              | `audit/audit-logger.ts`                                                     | `security.test.ts`、`secrets-scanner.test.ts`                                                       | 已验证  |
| 21–22      | 插件安装、迁移、目录与 manifest                     | `install/`、`scripts/install-*.mjs`、`packages/*-plugin/`                   | `install-scripts.test.ts`、`plugin-manifest.test.ts`                                                | 已验证  |
| 23.1、23.9 | init/status：空项目澄清、项目规范画像、状态恢复     | `commands/init.ts`、`status.ts`、`project-conventions/`                     | `init-status.test.ts`、`state.test.ts`、Adapter 契约测试                                            | 已验证  |
| 23.2       | new：澄清、OpenSpec model/delta、三件套保护         | `commands/new.ts`、`engines/spec/`、`codebase/codebase-adapter.ts`          | `new.test.ts`、`spec-engine.test.ts`、`mcp-transport.test.ts`、`prompt-injection-guard.test.ts`     | 已验证  |
| 23.3       | design：规格和真实代码结构                          | `commands/design.ts`、`TddEngine.generateDesign`                            | `design-plan.test.ts`                                                                               | 已验证  |
| 23.4       | plan：四阶段任务、依赖、精确范围、Context Pack      | `commands/plan.ts`、`engines/superpowers/`                                  | `superpowers-engine.test.ts`、`design-plan.test.ts`                                                 | 已验证  |
| 23.5       | build：范围、并行、TDD 证据、TaskExecutor v2 归一化 | `commands/build.ts`、`build/task-result-normalizer.ts`、`security/`         | `build.test.ts`、`task-executor-v2.test.ts`、`tdd-evidence.test.ts`                                 | 已验证  |
| 23.6       | verify：model 优先、Scenario 覆盖、漂移             | `commands/verify.ts`、`quality/verify-report.ts`                            | `verify-report.test.ts`、`quality-commands.test.ts`、`workflow.test.ts`、`worktree.test.ts`         | 已验证  |
| 23.7       | review：范围与持久化证据重验                        | `commands/review.ts`、`quality/review-report.ts`、`deterministic-review.ts` | `review-report.test.ts`、`quality-commands.test.ts`、`workflow.test.ts`                             | 已验证  |
| 23.8       | archive：报告、追踪、重验、原子组提交               | `commands/archive.ts`、`quality/traceability.ts`                            | `quality-commands.test.ts`、`artifact-writer.test.ts`、`workflow.test.ts`、`worktree.test.ts`       | 已验证  |
| 23.10      | auto：有界 Loop、resume/restart、BLOCKER 与失败恢复 | `Core.runAuto`、`commands/auto.ts`、`loop/`                                 | `auto.test.ts`、`loop.test.ts`、`workflow.test.ts`                                                  | 已验证  |
| 24.1–24.4  | Unit/Integration/E2E/Security/Regression 与示例仓库 | `packages/*/test`、`test/e2e`、`fixtures/`                                  | `npm test`、`validate-schemas.mjs`                                                                  | 已验证  |
| 24.5       | macOS/Windows × Claude Code/Codex                   | `.github/workflows/ci.yml`                                                  | `adapter-contract.test.ts`、`workflow.test.ts`（含 worktree E2E）；8 组合 CI matrix                 | CI 配置 |
| 26.1       | MVP0 插件、init/status、降级、原子状态、锁、Schema  | Core、插件 manifest、安装器                                                 | `init-status.test.ts`、`state.test.ts`、`acceptance.test.ts`                                        | 已验证  |
| 26.2       | MVP1 new/design/plan、Context Pack、人工修改保护    | Spec/TDD Engine、阶段命令                                                   | `new.test.ts`、`design-plan.test.ts`                                                                | 已验证  |
| 26.3       | MVP2 build/verify/review/archive/auto、只读         | Build、Quality、Archive、Auto、GitIsolation                                 | `build.test.ts`、`quality-commands.test.ts`、`auto.test.ts`、`worktree.test.ts`、E2E                | 已验证  |

本地完整验证命令：

```bash
npm run format:check
npm run lint
npm run typecheck
npm test
npm run test:coverage
npm run validate:schemas
npm run validate:release
git diff --check
```

Windows 与 macOS 的宿主组合证据由 `.github/workflows/ci.yml` 产生；本地测试结果不能替代远程 Windows runner 结果。

## 二期最终验收逐条核对

说明：`docs/需求文档.md` 第 26 章当前实际列出 21 条验收项，下面按文档原文逐条映射实现与自动化证据。

### 26.1 MVP0

| 条目   | 验收内容                                                | 实现位置                                                    | 自动化证据                                          |
| ------ | ------------------------------------------------------- | ----------------------------------------------------------- | --------------------------------------------------- |
| 26.1-1 | CLI 可通过安装脚本全局安装（macOS/Linux）               | `scripts/install.sh`                                        | `cli.test.ts`                                       |
| 26.1-2 | CLI 可通过安装脚本全局安装（Windows）                   | `scripts/install.ps1`                                       | `cli.test.ts`                                       |
| 26.1-3 | 两个平台的 init 命令都能生成 `.sdd/`                    | `commands/init.ts`、`state/state-store.ts`                  | `init-status.test.ts`、`acceptance.test.ts`         |
| 26.1-4 | 两个平台的 init 命令都能生成 `CLAUDE.md` 和 `AGENTS.md` | `install/project-installer.ts`、`packages/*-plugin/skills/` | `init-status.test.ts`、`acceptance.test.ts`         |
| 26.1-5 | `codebase-memory-mcp` 不可用时能降级                    | `codebase/codebase-adapter.ts`                              | `codebase-adapter.test.ts`、`acceptance.test.ts`    |
| 26.1-6 | 两个平台的 status 命令都能输出相同状态                  | `commands/status.ts`、两端 Adapter                          | `adapter-contract.test.ts`、`acceptance.test.ts`    |
| 26.1-7 | `state.json` 原子写入                                   | `state/state-store.ts`                                      | `state.test.ts`                                     |
| 26.1-8 | 并发写命令会被 lock 阻断                                | `state/file-lock.ts`、写命令入口                            | `state.test.ts`、`acceptance.test.ts`               |
| 26.1-9 | schema 校验可运行                                       | `schemas/`、`scripts/validate-schemas.mjs`                  | `schema-golden.test.ts`、`npm run validate:schemas` |

### 26.2 MVP1

| 条目   | 验收内容                                         | 实现位置                                    | 自动化证据                                                   |
| ------ | ------------------------------------------------ | ------------------------------------------- | ------------------------------------------------------------ |
| 26.2-1 | 两个平台的 new 命令都能生成一致的 `spec.md`      | `commands/new.ts`、两端 Adapter             | `new.test.ts`、`acceptance.test.ts`                          |
| 26.2-2 | BLOCKER 未回答不进入 design                      | `commands/new.ts`、`core.ts`                | `new.test.ts`、`acceptance.test.ts`                          |
| 26.2-3 | 两个平台的 design 命令都能生成一致的 `design.md` | `commands/design.ts`、两端 Adapter          | `design-plan.test.ts`、`acceptance.test.ts`                  |
| 26.2-4 | 两个平台的 plan 命令都能生成一致的 `tasks.md`    | `commands/plan.ts`、两端 Adapter            | `design-plan.test.ts`、`acceptance.test.ts`                  |
| 26.2-5 | 每个 Task 有 Context Pack                        | `build/context-pack.ts`、`commands/plan.ts` | `design-plan.test.ts`、`workflow.test.ts`                    |
| 26.2-6 | 重复执行不会静默覆盖人工修改                     | `ArtifactWriter`、阶段命令 candidate 逻辑   | `design-plan.test.ts`、`artifact-writing-regression.test.ts` |

### 26.3 MVP2

| 条目   | 验收内容                                      | 实现位置                                                  | 自动化证据                                          |
| ------ | --------------------------------------------- | --------------------------------------------------------- | --------------------------------------------------- |
| 26.3-1 | 两个平台的 build 命令都能按任务执行           | `commands/build.ts`、两端 Adapter                         | `build.test.ts`、`acceptance.test.ts`               |
| 26.3-2 | 两个平台的 verify 命令都能发现未完成需求      | `commands/verify.ts`、`quality/verify-report.ts`          | `quality-commands.test.ts`、`verify-report.test.ts` |
| 26.3-3 | 两个平台的 review 命令都能发现无关修改        | `commands/review.ts`、`quality/review-report.ts`          | `quality-commands.test.ts`、`review-report.test.ts` |
| 26.3-4 | 两个平台的 archive 命令都能生成一致的归档报告 | `commands/archive.ts`、`quality/traceability.ts`          | `quality-commands.test.ts`、`acceptance.test.ts`    |
| 26.3-5 | 两个平台的 auto 命令都能从粗略需求跑完整流程  | `core.ts`、`commands/auto.ts`、`loop/`                    | `auto.test.ts`、`workflow.test.ts`                  |
| 26.3-6 | ARCHIVED 后只读                               | `commands/archive.ts`、`commands/change-id.ts`、`core.ts` | `quality-commands.test.ts`、`acceptance.test.ts`    |
