# 需求追踪矩阵

| 需求领域                                   | 实现位置                                                                      | 证据                                                                        |
| ------------------------------------------ | ----------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| 共享 Core 与命令契约                       | `packages/core/src/core.ts`、`contracts.ts`                                   | `contracts.test.ts`                                                         |
| 原子状态、恢复、迁移、锁与状态写审计       | `packages/core/src/state/`                                                    | `state.test.ts`                                                             |
| MCP 与降级索引                             | `packages/core/src/codebase/`                                                 | `codebase-adapter.test.ts`、`codebase-adapter.real.test.ts`（本地显式开启） |
| 初始化、组件版本记录、完整性校验与状态查询 | `packages/core/src/commands/init.ts`、`status.ts`                             | `init-status.test.ts`、`dependency-integrity.test.ts`                       |
| SpecEngine 与 `new`                        | `packages/core/src/engines/spec/`、`commands/new.ts`                          | `spec-engine.test.ts`、`new.test.ts`                                        |
| TddEngine、设计与计划                      | `packages/core/src/engines/tdd/`、`commands/design.ts`、`plan.ts`             | `design-plan.test.ts`                                                       |
| Context Pack 元数据与新鲜度失效校验        | `packages/core/src/commands/plan.ts`、`build.ts`                              | `design-plan.test.ts`、`build.test.ts`                                      |
| 构建、文件范围、安全命令、失败重试         | `packages/core/src/build/`、`commands/build.ts`、`security/`                  | `build.test.ts`、`git-scope.test.ts`                                        |
| 验证、审查、归档、自动流转                 | `packages/core/src/quality/`、`commands/verify.ts`、`review.ts`、`archive.ts` | `quality-commands.test.ts`、`auto.test.ts`                                  |
| Claude Code 与 Codex 一致性                | `packages/*-plugin/`                                                          | `adapter-contract.test.ts`、`workflow.test.ts`                              |
| MVP0–MVP2 命令验收                         | `test/e2e/acceptance.test.ts`                                                 | Claude/Codex 手工阶段流与 BLOCKER 验收                                      |
| 插件 manifest、入口与 Core 兼容性          | `packages/*-plugin/.{claude,codex}-plugin/plugin.json`                        | `plugin-manifest.test.ts`                                                   |
| 发布验证                                   | `scripts/validate-release.mjs`                                                | `release-validation.test.ts`                                                |
| Schema 校验可运行                          | `scripts/validate-schemas.mjs`                                                | `schema-validation.test.ts`                                                 |
| 安装、升级、手工卸载说明                   | `README.md`、`docs/plugin-installation.md`                                    | 文档章节与迁移测试                                                          |
| macOS / Windows 路径安全                   | `security/path-safety.ts`                                                     | `security.test.ts`                                                          |
| 审计日志轮转与敏感信息脱敏                 | `audit/audit-logger.ts`                                                       | `security.test.ts`                                                          |
| 许可证与固定依赖版本                       | `dependencies.ts`、`THIRD_PARTY_NOTICES.md`、`vendor/`                        | `dependency-metadata.test.ts`                                               |
