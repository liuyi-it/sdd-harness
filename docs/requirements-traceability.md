# Requirements Traceability

| Requirement area                           | Implementation                                                                | Evidence                                       |
| ------------------------------------------ | ----------------------------------------------------------------------------- | ---------------------------------------------- |
| Shared Core and command contract           | `packages/core/src/core.ts`, `contracts.ts`                                   | `contracts.test.ts`                            |
| Atomic state, recovery, migration, locking | `packages/core/src/state/`                                                    | `state.test.ts`                                |
| MCP and fallback indexing                  | `packages/core/src/codebase/`                                                 | `codebase-adapter.test.ts`                     |
| Init and status                            | `packages/core/src/commands/init.ts`, `status.ts`                             | `init-status.test.ts`                          |
| SpecEngine and new                         | `packages/core/src/engines/spec/`, `commands/new.ts`                          | `spec-engine.test.ts`, `new.test.ts`           |
| TddEngine design and plan                  | `packages/core/src/engines/tdd/`, `commands/design.ts`, `plan.ts`             | `design-plan.test.ts`                          |
| Build, file scope, shell policy, retry     | `packages/core/src/build/`, `commands/build.ts`, `security/`                  | `build.test.ts`, `git-scope.test.ts`           |
| Verify, review, archive, auto              | `packages/core/src/quality/`, `commands/verify.ts`, `review.ts`, `archive.ts` | `quality-commands.test.ts`, `auto.test.ts`     |
| Claude Code and Codex parity               | `packages/*-plugin/`                                                          | `adapter-contract.test.ts`, `workflow.test.ts` |
| macOS and Windows path security            | `security/path-safety.ts`                                                     | `security.test.ts`                             |
| Audit logging and redaction                | `audit/audit-logger.ts`                                                       | `security.test.ts`                             |
| Licensing and pinned dependencies          | `dependencies.ts`, `THIRD_PARTY_NOTICES.md`, `vendor/`                        | `codebase-adapter.test.ts`                     |
