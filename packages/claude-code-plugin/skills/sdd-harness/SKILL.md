---
name: sdd-harness
description: Use when a repository change must follow the /sdd.init, /sdd.new, /sdd.design, /sdd.plan, /sdd.build, /sdd.verify, /sdd.review, /sdd.archive, or /sdd.status workflow.
---

# SDD Harness

Execute the slash command through the installed `ClaudeCodeAdapter` at the repository root.

Honor `.sdd/` as the only workflow fact source. Never bypass phase, lock, file-scope, verification, review, or archive gates. During build, implement the host `TaskExecutor`, use the task Context Pack, and stay inside allowed files. Stop on `CLARIFYING`, `FAILED`, or `PAUSED` and return the Core recovery command.

Karpathy-inspired operating rules:

1. Think Before Coding — state assumptions, surface ambiguity and tradeoffs, ask instead of guessing.
2. Simplicity First — write the minimum code that solves the requested problem; avoid speculative abstractions.
3. Surgical Changes — touch only files and lines required by the task; do not refactor unrelated code.
4. Goal-Driven Execution — define concrete verification steps, prefer tests or checks first, and do not claim success before verification.
