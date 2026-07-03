# 需求追踪矩阵

| 需求领域                           | 实现位置                                                                      | 证据                                           |
| ---------------------------------- | ----------------------------------------------------------------------------- | ---------------------------------------------- |
| 共享 Core 与命令契约               | `packages/core/src/core.ts`、`contracts.ts`                                   | `contracts.test.ts`                            |
| 原子状态、恢复、迁移、锁           | `packages/core/src/state/`                                                    | `state.test.ts`                                |
| MCP 与降级索引                     | `packages/core/src/codebase/`                                                 | `codebase-adapter.test.ts`                     |
| 初始化与状态查询                   | `packages/core/src/commands/init.ts`、`status.ts`                             | `init-status.test.ts`                          |
| SpecEngine 与 `new`                | `packages/core/src/engines/spec/`、`commands/new.ts`                          | `spec-engine.test.ts`、`new.test.ts`           |
| TddEngine、设计与计划              | `packages/core/src/engines/tdd/`、`commands/design.ts`、`plan.ts`             | `design-plan.test.ts`                          |
| 构建、文件范围、安全命令、失败重试 | `packages/core/src/build/`、`commands/build.ts`、`security/`                  | `build.test.ts`、`git-scope.test.ts`           |
| 验证、审查、归档、自动流转         | `packages/core/src/quality/`、`commands/verify.ts`、`review.ts`、`archive.ts` | `quality-commands.test.ts`、`auto.test.ts`     |
| Claude Code 与 Codex 一致性        | `packages/*-plugin/`                                                          | `adapter-contract.test.ts`、`workflow.test.ts` |
| macOS / Windows 路径安全           | `security/path-safety.ts`                                                     | `security.test.ts`                             |
| 审计日志与敏感信息脱敏             | `audit/audit-logger.ts`                                                       | `security.test.ts`                             |
| 许可证与固定依赖版本               | `dependencies.ts`、`THIRD_PARTY_NOTICES.md`、`vendor/`                        | `codebase-adapter.test.ts`                     |
