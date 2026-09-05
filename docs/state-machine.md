# 状态机

## 项目状态

项目级状态只有三个初始化阶段：

```text
NOT_INITIALIZED → INITIALIZING → INDEX_READY
```

完成初始化后，项目级 state 保持 `INDEX_READY`；具体进度由每个 change 的 workflow 保存。

## Change 状态

```text
SPEC_WAITING_AGENT → SPEC_READY
  → PLAN_WAITING_AGENT → PLAN_READY
  → BUILD_WAITING_AGENT ↔ PLAN_READY → BUILD_READY
  → QUALITY_WAITING_FIX → QUALITY_READY
                       ↘ QUALITY_BLOCKED
  → ARCHIVED
```

- `*_WAITING_AGENT`：Core 已准备上下文和 Schema，等待宿主回传 inline JSON。
- 等待中的规格、计划和质量修复可用状态建议的原命令恢复行动；构建使用 `build next` 恢复同一任务，无需用户手工拼装 JSON。
- `PLAN_READY`：存在可派发或可重试的纵向任务。
- `BUILD_WAITING_AGENT`：恰好一个任务为 BUILDING，pendingAgentAction 必须指向同一 taskId。
- `BUILD_READY`：所有计划任务为 DONE。
- `QUALITY_WAITING_FIX`：统一质量门禁失败，等待当前受控修复结果。
- `QUALITY_BLOCKED`：自动修复预算已用完，必须询问用户；明确授权后 `sdd verify --continue` 开启下一轮。
- `QUALITY_BLOCKED` 下仍可手动修复并运行普通 `sdd verify`；若重新评估通过，直接进入 `QUALITY_READY`，无需开启额外 Agent 修复轮次。
- `QUALITY_READY`：统一质量报告通过，可归档。
- `ARCHIVED`：只读终态；`change` 可显式修订并重新进入规格阶段。

## 多任务选择

活动任务是 phase 不为 `ARCHIVED` 的 workflow。

- 0 个：阶段命令返回 `E_MISSING_CHANGE`。
- 1 个：未传 `--change` 时可唯一解析。
- 多个：任何需要 change 的命令返回 `E_CHANGE_SELECTION_REQUIRED`；宿主必须展示候选并询问用户。
- `status`：不触发错误，返回 `MULTIPLE_CHANGES` 和全部 `activeChanges`。

禁止依据更新时间、创建时间、目录顺序或“最近使用”自动选择。

## 修订与归档

`sdd change <新需求> --change <id>` 更新 run 的原始需求并进入 `SPEC_WAITING_AGENT`。统一规格完成时，旧 plan、reports、archive、任务结果和对应制品索引同时作废；唯一 `spec.md` 同时承载需求规格与技术设计，Git 负责历史。

`PLAN_WAITING_AGENT` 允许修订。已在等待修订结果时，非空输入更新为最新需求，不带需求则恢复当前行动。修订行动提供此前完整规格，宿主据此保留仍有效的需求和设计。

修订等待期间，状态与选择候选显示最新需求标题；阶段错误建议保留目标标识。状态查询保持只读，质量阻断时可直接查看当前问题。

归档前会重新比较质量报告中的 Git 指纹；验证后发生新改动时退回 `BUILD_READY`，必须重新 verify。归档生成 `archive.md` 后删除该 change 的其他人读文档，但机器模型保留在 Runtime。

`tasks.md` 展示计划定义，实时进度由 status 提供；归档记录已完成任务数，不复制不会更新的未完成复选框。
