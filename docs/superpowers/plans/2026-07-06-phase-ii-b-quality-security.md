# Phase II-B Quality And Security Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 接入固定版 codebase-memory-mcp 查询、机器可读质量门禁、Secrets Scanner 与提示注入防护。

**Architecture:** CodebaseAdapter 通过 McpTransportV2 能力协商并统一返回 query result；quality 模块只消费 Core 已验证事实；security 模块在上下文生成、制品写入和 review 三个边界执行防护。

**Tech Stack:** TypeScript、Zod、JSON Schema、Vitest、codebase-memory-mcp v0.8.1 (`f0c9be1`)

---

### Task 1: MCP capability discovery 与统一查询

**Files:**

- Modify: `packages/core/src/codebase/codebase-adapter.ts`
- Create: `packages/core/src/codebase/mcp-query.ts`
- Modify: `packages/core/src/commands/init.ts`
- Modify: `packages/core/src/commands/new.ts`
- Create: `schemas/mcp-query-result.schema.json`
- Test: `packages/core/test/mcp-transport.test.ts`

- [ ] 写失败测试，覆盖 capabilities/diagnostics 落盘、impact query、缺工具/超时 fallback。
- [ ] 运行 `npx vitest run packages/core/test/mcp-transport.test.ts`，预期失败。
- [ ] 定义 `McpCapabilities`、`McpQueryInput`、`McpQueryResult`；provider 只允许 codebase-memory-mcp/fallback-file-scan。
- [ ] 实现 V2 transport 与 fallback 统一结构；fallback confidence 低于精确结果且包含 reason。
- [ ] new 以 `intent: "impact"` 查询并将摘要写入 impact 制品。
- [ ] 运行目标测试，预期通过。
- [ ] 提交：`git commit -m "feat: 接入 MCP 能力发现与结构化查询"`。

### Task 2: VerifyReport v1.2

**Files:**

- Create: `packages/core/src/quality/verify-report.ts`
- Modify: `packages/core/src/commands/verify.ts`
- Create: `schemas/verify-report.schema.json`
- Test: `packages/core/test/verify-report.test.ts`

- [ ] 写失败测试，断言 PASS/FAIL 都先生成 JSON/Markdown，缺场景证据时 JSON 可读且返回 `E_VERIFY_FAILED`。
- [ ] 运行目标测试确认失败。
- [ ] 实现 `VerifyReport`，levels 固定 artifacts/tasks/requirements/scenarios/tddEvidence/tests/drift，failure 使用稳定 code 和可选实体字段。
- [ ] verify 先原子写报告，再推进或失败；写报告失败不得伪装为业务失败报告。
- [ ] 运行目标测试及 `packages/core/test/quality-commands.test.ts`，预期通过。
- [ ] 提交：`git commit -m "feat: 生成机器可读验证报告"`。

### Task 3: ReviewReport 与确定性审查

**Files:**

- Create: `packages/core/src/quality/review-report.ts`
- Create: `packages/core/src/quality/deterministic-review.ts`
- Modify: `packages/core/src/commands/review.ts`
- Create: `schemas/review-issue.schema.json`
- Create: `schemas/review-report.schema.json`
- Test: `packages/core/test/review-report.test.ts`

- [ ] 写失败测试，覆盖 FILE_SCOPE、UNRELATED_CHANGE、SECURITY、TESTING 及 severity 汇总。
- [ ] 运行目标测试确认失败。
- [ ] 实现 `ReviewIssue`/`ReviewReport`，ID 由 category/file/task/message 的稳定哈希生成。
- [ ] 实现 archive 阻断规则：BLOCKER、SECRET_LEAK、SECURITY/FILE_SCOPE MAJOR、forbidden 和 untracked diff。
- [ ] 运行目标测试与 archive 回归测试，预期通过。
- [ ] 提交：`git commit -m "feat: 添加确定性结构化审查"`。

### Task 4: Secrets Scanner 与审计脱敏

**Files:**

- Create: `packages/core/src/security/secrets-scanner.ts`
- Modify: `packages/core/src/audit/audit-logger.ts`
- Modify: `packages/core/src/artifacts/artifact-writer.ts`
- Modify: `packages/core/src/commands/review.ts`
- Test: `packages/core/test/secrets-scanner.test.ts`

- [ ] 写失败测试，覆盖 AWS Key、GitHub token、JWT、私钥、Authorization、数据库密码和 generic secret；断言报告/audit 不含原值。
- [ ] 运行目标测试确认失败。
- [ ] 实现 `SecretFinding`，preview 只保留类型和首尾最多 2 字符；扫描 current-run diff、任务结果和待写制品。
- [ ] 实现“路径 + 规则类型”例外；private-key/github-token 不允许例外，所有例外写审计。
- [ ] AuditLogger 在序列化前递归脱敏 token/password/secret/api key/authorization/private key/database URL。
- [ ] 运行目标测试，预期通过且输出中无 fixture 原始 secret。
- [ ] 提交：`git commit -m "feat: 阻断敏感信息泄露"`。

### Task 5: Prompt Injection Guard

**Files:**

- Create: `packages/core/src/security/untrusted-content.ts`
- Modify: `packages/core/src/commands/new.ts`
- Modify: `packages/core/src/commands/plan.ts`
- Modify: `packages/core/src/build/task-executor.ts`
- Test: `packages/core/test/prompt-injection-guard.test.ts`

- [ ] 写失败测试：README 含恶意指令时，impact/context/context-pack 用边界包裹，allowedCommands 不增加危险命令。
- [ ] 运行目标测试确认失败。
- [ ] 实现 `wrapUntrustedRepositoryContent` 与 `wrapUntrustedMcpOutput`，拒绝内容伪造结束标记并进行转义。
- [ ] Adapter 固定注入六条安全规则；TaskExecutor 请求只能引用结构化 constraints。
- [ ] 运行目标测试及 build/new 回归测试，预期通过。
- [ ] 提交：`git commit -m "feat: 增加不可信上下文边界"`。

### Task 6: Golden E2E、文档与 II-B 门禁

**Files:**

- Modify: `scripts/validate-schemas.mjs`
- Modify: `test/e2e/workflow.test.ts`
- Modify: `docs/security.md`
- Modify: `docs/schemas.md`
- Modify: `docs/architecture.md`
- Modify: `docs/requirements-traceability.md`

- [ ] E2E 运行真实 auto 并校验 loop/task/verify/review/MCP 制品；增加失败报告、secret 和 injection 场景。
- [ ] validate:schemas 读取 E2E 真实输出，不以手写对象替代。
- [ ] 更新中文安全、Schema、架构和追踪文档。
- [ ] 运行完整质量门禁，预期全部退出 0；再运行 `git diff --check`。
- [ ] 提交：`git commit -m "test: 完成二期 B 阶段质量验收"`。
- [ ] 推送并确认远端提交后，才开始 II-C。
