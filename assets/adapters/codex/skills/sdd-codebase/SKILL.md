---
name: sdd-codebase
description: 用户要查看、诊断、索引或查询 SDD 使用的代码库上下文时使用。
---

# SDD Codebase

根据意图运行 `sdd codebase status|doctor|index|query|rebuild --json`。`query` 必须带非空查询。该命令是项目级操作，不需要选择 change；只汇报索引提供方、是否降级和查询结论，不展示内部原始上下文。
