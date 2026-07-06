# 需求追踪矩阵

状态说明：`已验证` 表示存在实现、行为测试和可重复执行的验证命令；`CI 配置` 表示组合矩阵已经定义，实际运行结果以 GitHub Actions 为准。

| 需求章节   | 验收内容                                            | 实现位置                                                             | 自动化证据                                                                         | 状态    |
| ---------- | --------------------------------------------------- | -------------------------------------------------------------------- | ---------------------------------------------------------------------------------- | ------- |
| 2.4        | Claude/Codex 共享确定性 Core                        | `packages/core/src/core.ts`、`contracts.ts`、两端 Adapter            | `contracts.test.ts`、`adapter-contract.test.ts`                                    | 已验证  |
| 3.1        | `.sdd/` 是唯一运行时事实源                          | `commands/*`、`ArtifactWriter`、`StateStore`                         | `artifact-writing-regression.test.ts`、`workflow.test.ts`                          | 已验证  |
| 3.2、4.1   | 原版 codebase-memory-mcp 与降级扫描                 | `codebase/codebase-adapter.ts`                                       | `codebase-adapter.test.ts`、可选 `codebase-adapter.real.test.ts`                   | 已验证  |
| 3.3、4.2   | OpenSpec 固定快照与内置 SpecEngine                  | `vendor/openspec/`、`engines/openspec/`、`engines/spec/`             | `openspec-engine.test.ts`、`spec-engine.test.ts`、`dependency-metadata.test.ts`    | 已验证  |
| 3.4、4.3   | Superpowers 固定快照与内置 TDD 工作流               | `vendor/superpowers/`、`engines/superpowers/`、`engines/tdd/`        | `superpowers-engine.test.ts`、`design-plan.test.ts`、`dependency-metadata.test.ts` | 已验证  |
| 3.5、17.7  | 许可证、版本、commit、逐文件清单                    | `vendor/*/VERSION.json`、`MANIFEST.sha256`、`THIRD_PARTY_NOTICES.md` | `dependency-metadata.test.ts`、`release-validation.test.ts`                        | 已验证  |
| 7          | 参数、help/version、输出、错误与退出码              | `adapters/host-adapter.ts`、`contracts.ts`                           | `contracts.test.ts`、`adapter-contract.test.ts`                                    | 已验证  |
| 8–12       | 状态机、恢复、幂等与 candidate                      | `state/`、`commands/recovery.ts`、`ArtifactWriter`                   | `state.test.ts`、`artifact-writer.test.ts`、各命令测试                             | 已验证  |
| 13–14      | 原子 state、迁移、全局锁与并发                      | `state/state-store.ts`、`file-lock.ts`                               | `state.test.ts`、`acceptance.test.ts`                                              | 已验证  |
| 15         | 归档后只读与 marker/state 收敛                      | `commands/archive.ts`、`commands/change-id.ts`                       | `quality-commands.test.ts`                                                         | 已验证  |
| 16         | Git baseline、精确文件范围、命令白名单              | `git/`、`security/`、`commands/build.ts`                             | `git-scope.test.ts`、`build.test.ts`、`scope-overlap.test.ts`                      | 已验证  |
| 17         | 提示注入、MCP 信任边界、脱敏与路径安全              | `codebase/`、`audit/`、`security/`                                   | `security.test.ts`、`workflow.test.ts`                                             | 已验证  |
| 18         | 配置、状态、任务与 metadata Schema                  | `schemas/`、`install/canonical-schemas.ts`                           | `schema-validation.test.ts`、`npm run validate:schemas`                            | 已验证  |
| 19         | Context Pack 大小、hash 与新鲜度                    | `commands/plan.ts`、`commands/build.ts`                              | `design-plan.test.ts`、`build.test.ts`                                             | 已验证  |
| 20         | JSONL 审计、轮转与脱敏                              | `audit/audit-logger.ts`                                              | `security.test.ts`                                                                 | 已验证  |
| 21–22      | 插件安装、迁移、目录与 manifest                     | `install/`、`scripts/install-*.mjs`、`packages/*-plugin/`            | `install-scripts.test.ts`、`plugin-manifest.test.ts`                               | 已验证  |
| 23.1、23.9 | init/status                                         | `commands/init.ts`、`status.ts`                                      | `init-status.test.ts`、Adapter 契约测试                                            | 已验证  |
| 23.2       | new：澄清、OpenSpec model/delta、三件套保护         | `commands/new.ts`、`engines/spec/`                                   | `new.test.ts`、`spec-engine.test.ts`                                               | 已验证  |
| 23.3       | design：规格和真实代码结构                          | `commands/design.ts`、`TddEngine.generateDesign`                     | `design-plan.test.ts`                                                              | 已验证  |
| 23.4       | plan：四阶段任务、依赖、精确范围、Context Pack      | `commands/plan.ts`、`engines/superpowers/`                           | `superpowers-engine.test.ts`、`design-plan.test.ts`                                | 已验证  |
| 23.5       | build：范围、并行、TDD 证据、部分重试               | `commands/build.ts`、`quality/tdd-evidence.ts`                       | `build.test.ts`、`tdd-evidence.test.ts`                                            | 已验证  |
| 23.6       | verify：model 优先、Scenario 覆盖、漂移             | `commands/verify.ts`、`quality/`                                     | `quality-commands.test.ts`                                                         | 已验证  |
| 23.7       | review：范围与持久化证据重验                        | `commands/review.ts`、`reviewGate`                                   | `quality-commands.test.ts`                                                         | 已验证  |
| 23.8       | archive：报告、追踪、重验、原子组提交               | `commands/archive.ts`、`quality/traceability.ts`                     | `quality-commands.test.ts`、`artifact-writer.test.ts`                              | 已验证  |
| 23.10      | auto：顺序编排、BLOCKER 与失败恢复                  | `Core.runAuto`                                                       | `auto.test.ts`、`workflow.test.ts`                                                 | 已验证  |
| 24.1–24.4  | Unit/Integration/E2E/Security/Regression 与示例仓库 | `packages/*/test`、`test/e2e`、`fixtures/`                           | `npm test`                                                                         | 已验证  |
| 24.5       | macOS/Windows × Claude Code/Codex                   | `.github/workflows/ci.yml`                                           | `adapter-contract.test.ts`、`workflow.test.ts`；8 组合 CI matrix                   | CI 配置 |
| 26.1       | MVP0 插件、init/status、降级、原子状态、锁、Schema  | Core、插件 manifest、安装器                                          | `init-status.test.ts`、`state.test.ts`、`acceptance.test.ts`                       | 已验证  |
| 26.2       | MVP1 new/design/plan、Context Pack、人工修改保护    | Spec/TDD Engine、阶段命令                                            | `new.test.ts`、`design-plan.test.ts`                                               | 已验证  |
| 26.3       | MVP2 build/verify/review/archive/auto、只读         | Build、Quality、Archive、Auto                                        | `build.test.ts`、`quality-commands.test.ts`、`auto.test.ts`、E2E                   | 已验证  |

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
