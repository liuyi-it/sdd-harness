# Chinese Comments And Docs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 `packages/core/src` 和 `packages/*/test` 补充高质量中文注释，并将仓库主文档改为中文优先表述。

**Architecture:** 不改变现有行为和接口，只补充文件职责说明、关键流程注释、测试意图说明，并把现有英文文档页翻译为中文。注释重点放在状态机、恢复逻辑、并发锁、构建阶段、安全校验与测试意图，避免逐行翻译造成噪音。

**Tech Stack:** TypeScript、Vitest、Markdown、Prettier、ESLint

---

### Task 1: 中文化文档页

**Files:**

- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/command-contract.md`
- Modify: `docs/plugin-installation.md`
- Modify: `docs/requirements-traceability.md`
- Modify: `docs/schemas.md`
- Modify: `docs/security.md`
- Modify: `docs/state-machine.md`

- [ ] 将英文说明改成中文优先，保留必要英文术语原名
- [ ] 保持现有结构和信息不丢失，不扩写无关内容

### Task 2: 为核心实现补中文职责注释

**Files:**

- Modify: `packages/core/src/**/*.ts`

- [ ] 为每个核心文件补充文件级中文职责说明
- [ ] 在复杂流程处补充中文注释：状态流转、恢复、并发、幂等、安全、构建批处理、归档与审计
- [ ] 不对显而易见的赋值和导入写注释

### Task 3: 为测试补中文意图说明

**Files:**

- Modify: `packages/core/test/*.ts`
- Modify: `packages/adapters-test/adapter-contract.test.ts`

- [ ] 为测试文件补充文件级中文说明
- [ ] 为关键 `describe`/复杂用例补充中文注释，说明覆盖的行为和回归风险
- [ ] 不改变断言语义和测试结构

### Task 4: 运行回归校验

**Files:**

- Verify only

- [ ] 运行 `npm run format:check`
- [ ] 运行 `npm run lint`
- [ ] 运行 `npm run typecheck`
- [ ] 运行 `npm test`
