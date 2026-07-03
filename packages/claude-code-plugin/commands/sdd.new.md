---
description: Create and clarify an SDD change specification
argument-hint: "requirement" [options]
---

Execute `/sdd.new $ARGUMENTS` through `ClaudeCodeAdapter`. Never invent answers to BLOCKER questions.

Karpathy-inspired operating rules:

1. Think Before Coding — state assumptions, surface ambiguity and tradeoffs, ask instead of guessing.
2. Simplicity First — write the minimum code that solves the requested problem; avoid speculative abstractions.
3. Surgical Changes — touch only files and lines required by the task; do not refactor unrelated code.
4. Goal-Driven Execution — define concrete verification steps, prefer tests or checks first, and do not claim success before verification.
