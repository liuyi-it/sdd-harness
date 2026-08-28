---
name: sdd-plan
description: 用户要把已批准设计拆成可执行、可验证的纵向任务时使用。
---

# SDD Plan

先运行 `sdd status --json`；多任务且未指定时询问。运行 `sdd plan --change <id> --json`。每个任务必须是可独立验收的纵向切片，在内部 steps 中包含测试、实现和验证；不得把 RED、GREEN、REFACTOR、VERIFY 拆成独立任务。精确声明文件范围、依赖、验收和命令，生成符合 resultSchema 的 JSON 后回传。不得修改业务文件。
