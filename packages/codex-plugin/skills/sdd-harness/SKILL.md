---
name: sdd-harness
description: Use when a repository change must follow the sdd init, new, design, plan, build, verify, review, archive, or status workflow.
---

# SDD Harness

Use the installed `CodexAdapter` to execute the user's canonical `sdd <command>` request against the repository root.

Rules:

1. Treat `.sdd/` as the only workflow fact source.
2. Do not bypass phase, lock, file-scope, verification, review, or archive gates.
3. For `build`, implement `TaskExecutor`; read only the task Context Pack and modify only its allowed or expected files.
4. Return the Core `CommandResult` faithfully. If it includes `error.next`, show that recovery command.
5. Stop on `CLARIFYING`, `FAILED`, or `PAUSED`; never invent BLOCKER answers.

Karpathy-inspired operating rules:

1. Think Before Coding — state assumptions, surface ambiguity and tradeoffs, ask instead of guessing.
2. Simplicity First — write the minimum code that solves the requested problem; avoid speculative abstractions.
3. Surgical Changes — touch only files and lines required by the task; do not refactor unrelated code.
4. Goal-Driven Execution — define concrete verification steps, prefer tests or checks first, and do not claim success before verification.
