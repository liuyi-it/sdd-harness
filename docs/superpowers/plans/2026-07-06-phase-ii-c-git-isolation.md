# Phase II-C Git Isolation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 build/verify/review 提供安全的 branch/worktree 业务文件隔离，同时保持主项目 `.sdd/` 为唯一流程事实源。

**Architecture:** GitIsolationManager 根据配置创建或复用分支和 worktree，并返回 `ExecutionWorkspace`。阶段命令显式区分 controlRoot 与 businessRoot；所有 `.sdd/` 写入 controlRoot，GitInspector 和 TaskExecutor 使用 businessRoot。

**Tech Stack:** TypeScript、Node.js child_process（无 shell）、Git、Vitest

---

### Task 1: Git 隔离模型与安全命令执行

**Files:**

- Create: `packages/core/src/git-isolation/model.ts`
- Create: `packages/core/src/git-isolation/git-runner.ts`
- Create: `packages/core/src/git-isolation/manager.ts`
- Modify: `schemas/config.schema.json`
- Test: `packages/core/test/worktree.test.ts`

- [ ] 写失败测试：createWorktree 自动启用 branch；命令通过 execFile argv 执行；非法 changeId、仓库外路径和符号链接路径被阻断。
- [ ] 运行目标测试确认失败。
- [ ] 定义：

```ts
interface ExecutionWorkspace {
  controlRoot: string;
  businessRoot: string;
  branchName: string | null;
  worktreePath: string | null;
  baselineCommit: string;
}
```

- [ ] 实现 GitRunner，只允许预定义的 `rev-parse/status/branch/worktree` argv，不接受 shell 字符串。
- [ ] 实现 manager 路径与分支模式校验；配置加入 createBranch/createWorktree/branchPattern/worktreeDir。
- [ ] 运行目标测试，预期通过。
- [ ] 提交：`git commit -m "feat: 添加 Git 隔离管理器"`。

### Task 2: 创建、复用与冲突阻断

**Files:**

- Modify: `packages/core/src/git-isolation/manager.ts`
- Modify: `packages/core/src/state/state-store.ts`
- Test: `packages/core/test/worktree.test.ts`

- [ ] 写失败测试：新建、同基线复用、错误基线、已占用 worktree、脏 worktree 和重复执行。
- [ ] 运行目标测试确认失败。
- [ ] 创建 `sdd/<change-id>` 并在 `.sdd/worktrees/<change-id>` 建立 worktree；已存在时验证 branch、HEAD 和注册路径完全匹配。
- [ ] 冲突时返回稳定错误，不执行 reset/checkout -f/clean；state 记录 branchName/worktreePath/baselineCommit。
- [ ] 运行目标测试，预期通过。
- [ ] 提交：`git commit -m "feat: 安全创建和复用 SDD worktree"`。

### Task 3: 阶段命令接入双根目录

**Files:**

- Modify: `packages/core/src/commands/build.ts`
- Modify: `packages/core/src/commands/verify.ts`
- Modify: `packages/core/src/commands/review.ts`
- Modify: `packages/core/src/commands/archive.ts`
- Modify: `packages/core/src/build/task-executor.ts`
- Modify: `packages/core/src/git/git-inspector.ts`
- Test: `packages/core/test/worktree.test.ts`
- Test: `packages/core/test/build.test.ts`

- [ ] 写失败测试：TaskExecutor.root 指向 businessRoot；主工作区业务文件不变；state/report 仍写 controlRoot。
- [ ] 运行目标测试确认失败。
- [ ] 所有阶段显式传递 `ExecutionWorkspace`，禁止通过字符串替换推导 `.sdd` 路径。
- [ ] GitInspector 对 businessRoot 计算 delta；规则解析从 controlRoot 与 businessRoot 的适用规则生成快照。
- [ ] archive 报告记录 worktreePath、branchName 和最终 HEAD，但不删除、merge 或 push。
- [ ] 运行目标测试与完整 build/quality 回归，预期通过。
- [ ] 提交：`git commit -m "feat: 在隔离工作区执行质量流程"`。

### Task 4: 双平台 E2E 与故障恢复

**Files:**

- Modify: `test/e2e/workflow.test.ts`
- Modify: `packages/adapters-test/adapter-contract.test.ts`
- Modify: `.github/workflows/ci.yml`

- [ ] 增加 Claude Code/Codex worktree E2E：创建、build、verify、review、archive、恢复和冲突阻断。
- [ ] 使用 Node 路径 API，测试 Windows 分隔符与路径含空格；不依赖 POSIX shell。
- [ ] CI 保持 macOS/Windows、Node 20/22 矩阵并执行 worktree E2E。
- [ ] 运行目标测试，预期两个 Adapter 产生一致的 workspace 元数据。
- [ ] 提交：`git commit -m "test: 覆盖双平台 Git 隔离流程"`。

### Task 5: 文档、全量回归与最终交付

**Files:**

- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/security.md`
- Modify: `docs/state-machine.md`
- Modify: `docs/requirements-traceability.md`
- Modify: `docs/二期需求文档.md`

- [ ] 更新中文配置、生命周期、故障处理和“不自动清理/合并/推送”说明。
- [ ] 逐条核对二期需求 20 项验收标准，在追踪文档中链接测试和实现文件。
- [ ] 运行 `npm run format:check && npm run lint && npm run typecheck && npm test && npm run validate:schemas && npm run validate:release`，预期全部退出 0。
- [ ] 运行 `git diff --check` 和 `git status --short`，确认只有计划内文件。
- [ ] 提交：`git commit -m "docs: 完成二期 Git 隔离交付"`。
- [ ] 推送当前分支；确认远端 commit 与本地一致后，二期才可标记完成。
