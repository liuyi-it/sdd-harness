---
name: sdd-harness
description: Use when a repository change must follow the /sdd.init, /sdd.new, /sdd.design, /sdd.plan, /sdd.build, /sdd.verify, /sdd.review, /sdd.archive, or /sdd.status workflow.
---

# SDD Harness

Execute the slash command through the installed `ClaudeCodeAdapter` at the repository root.

Honor `.sdd/` as the only workflow fact source. Never bypass phase, lock, file-scope, verification, review, or archive gates. During build, implement the host `TaskExecutor`, use the task Context Pack, and stay inside allowed files. Stop on `CLARIFYING`, `FAILED`, or `PAUSED` and return the Core recovery command.
