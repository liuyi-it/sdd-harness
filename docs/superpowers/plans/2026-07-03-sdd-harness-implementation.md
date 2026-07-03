# sdd-harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement every MVP0–MVP2 capability and invariant defined in `需求文档.md` for Claude Code and Codex on macOS and Windows.

**Architecture:** An npm workspace contains a deterministic `@sdd-harness/core` package plus thin Claude Code and Codex adapters. The Core owns command dispatch, state transitions, locking, atomic persistence, schemas, artifact generation, Git/file safety, MCP abstraction, quality gates, and audit logs; adapters only normalize host input and render `CommandResult`.

**Tech Stack:** TypeScript 5.x, Node.js 20+, npm workspaces, Vitest, Zod, YAML.

---

### Task 1: Workspace and contracts

**Files:**

- Create: `package.json`, `tsconfig.base.json`, `vitest.config.ts`
- Create: `packages/core/package.json`, `packages/core/src/contracts.ts`, `packages/core/src/errors.ts`
- Test: `packages/core/test/contracts.test.ts`

- [ ] Write failing tests for command names, phases, exit codes, and result shapes.
- [ ] Run the focused test and verify RED because Core contracts do not exist.
- [ ] Implement typed contracts and canonical error mapping.
- [ ] Run focused tests and typecheck to verify GREEN.

### Task 2: State persistence, schemas, lock, audit, and path safety

**Files:**

- Create: `packages/core/src/state/*`, `packages/core/src/security/*`, `packages/core/src/audit/*`
- Create: `schemas/config.schema.json`, `schemas/state.schema.json`, `schemas/task.schema.json`, `schemas/artifact-metadata.schema.json`
- Test: `packages/core/test/state.test.ts`, `packages/core/test/security.test.ts`

- [ ] Write failing tests for atomic state writes, backup recovery, migration, exclusive lock, stale lock recovery, audit rotation/redaction, and path traversal/symlink blocking.
- [ ] Verify RED for missing implementations.
- [ ] Implement minimal persistence and safety services.
- [ ] Verify GREEN on macOS and simulate Windows path cases with `path.win32`.

### Task 3: MCP and fallback codebase adapter

**Files:**

- Create: `packages/core/src/codebase/*`, `packages/core/src/dependencies.ts`
- Test: `packages/core/test/codebase-adapter.test.ts`

- [ ] Write failing tests for pinned dependency metadata, MCP availability/index calls, degraded file-scan fallback, and generated index summaries.
- [ ] Verify RED.
- [ ] Implement an injectable MCP transport plus deterministic fallback scanner.
- [ ] Verify GREEN.

### Task 4: Init and status commands

**Files:**

- Create: `packages/core/src/commands/init.ts`, `packages/core/src/commands/status.ts`, `packages/core/src/core.ts`
- Test: `packages/core/test/init-status.test.ts`

- [ ] Write failing tests for `.sdd/` layout, config/state creation, idempotent init, missing-file repair, degraded status, JSON output, and next-command calculation.
- [ ] Verify RED.
- [ ] Implement init/status through Core dispatch with lock and audit integration.
- [ ] Verify GREEN.

### Task 5: SpecEngine and new

**Files:**

- Create: `packages/core/src/engines/spec/*`, `packages/core/src/commands/new.ts`
- Test: `packages/core/test/new.test.ts`, `packages/core/test/spec-engine.test.ts`

- [ ] Write failing tests for proposal/impact/questions/answers/assumptions/spec generation, BLOCKER pause, non-interactive failure, metadata hashes, and active-change protection.
- [ ] Verify RED.
- [ ] Implement the internal OpenSpec-derived model and generators without invoking OpenSpec at runtime.
- [ ] Verify GREEN.

### Task 6: TddEngine design and plan

**Files:**

- Create: `packages/core/src/engines/tdd/*`, `packages/core/src/commands/design.ts`, `packages/core/src/commands/plan.ts`
- Test: `packages/core/test/design-plan.test.ts`

- [ ] Write failing tests for `DESIGNING`/`PLANNING` transitions, design sections, requirement-linked tasks, dependency graph validation, allowed files, verification commands, and per-task Context Packs.
- [ ] Verify RED.
- [ ] Implement design and planning generators plus candidate overwrite protection.
- [ ] Verify GREEN.

### Task 7: Build execution and Git/file scope gates

**Files:**

- Create: `packages/core/src/commands/build.ts`, `packages/core/src/build/*`, `packages/core/src/git/*`
- Test: `packages/core/test/build.test.ts`, `packages/core/test/git-scope.test.ts`

- [ ] Write failing tests for dependency ordering, parallel-safe grouping, sequential conflict fallback, task states, allowed-file enforcement, baseline diff separation, shell approval, cancellation, timeout, and partial retry.
- [ ] Verify RED.
- [ ] Implement injectable task executor and deterministic file/Git gates.
- [ ] Verify GREEN.

### Task 8: Verify, review, archive, and auto

**Files:**

- Create: `packages/core/src/commands/verify.ts`, `review.ts`, `archive.ts`, `auto.ts`
- Create: `packages/core/src/quality/*`
- Test: `packages/core/test/quality-commands.test.ts`, `packages/core/test/auto.test.ts`

- [ ] Write failing tests for requirement/task/acceptance coverage, verification evidence, drift, unrelated changes, security findings, traceability, archive readonly, auto orchestration, pause, and recovery.
- [ ] Verify RED.
- [ ] Implement gates, reports, archive protection, and orchestration.
- [ ] Verify GREEN.

### Task 9: Claude Code and Codex plugins

**Files:**

- Create: `packages/claude-code-plugin/**`, `packages/codex-plugin/**`
- Test: `packages/adapters-test/adapter-contract.test.ts`

- [ ] Write failing adapter parity tests for all ten commands, options, help/version, results, and errors.
- [ ] Verify RED.
- [ ] Implement manifests, commands, skills, project instruction templates, and thin Core adapters.
- [ ] Verify GREEN.

### Task 10: Installation, migration, fixtures, and E2E

**Files:**

- Create: `packages/core/src/install/*`, `fixtures/*`, `test/e2e/*`
- Test: `test/e2e/workflow.test.ts`, `test/e2e/security.test.ts`

- [ ] Write failing E2E tests for macOS/Windows path semantics, install/re-init/upgrade/manual uninstall docs, mock MCP, full manual workflow, auto workflow, corrupted state, malicious repository content, and concurrent commands.
- [ ] Verify RED.
- [ ] Implement remaining installation and migration behavior.
- [ ] Verify GREEN.

### Task 11: Documentation, licensing, and completion audit

**Files:**

- Modify: `README.md`
- Create: `docs/architecture.md`, `docs/state-machine.md`, `docs/security.md`, `docs/plugin-installation.md`, `docs/command-contract.md`, `docs/schemas.md`, `THIRD_PARTY_NOTICES.md`
- Create: `docs/requirements-traceability.md`

- [ ] Document exact packages, commands, schemas, security model, pinned upstream versions, and licenses.
- [ ] Map every numbered requirement and acceptance item in `需求文档.md` to code and tests.
- [ ] Run formatter, lint, typecheck, unit, integration, E2E, coverage, package, and manifest validation.
- [ ] Inspect generated fixtures and complete the requirement-by-requirement audit with no missing evidence.
