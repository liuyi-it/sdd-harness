---
name: sdd-plan
description: 把已批准设计拆成可执行、可验证的纵向任务时使用。
---

# SDD Plan

先运行 status；多任务且未指定时询问。运行 `sdd plan --change <id> --json`。每个任务是完整纵向切片，测试、实现和验证放在内部 steps，不拆成 RED/GREEN/REFACTOR/VERIFY 四个任务；精确声明范围、依赖、验收和命令后回传 resultSchema JSON。
