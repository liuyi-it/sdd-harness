---
name: sdd-plan
description: 用户要把已批准的统一规格拆成可执行、可验证的纵向任务时使用。
---

# SDD Plan

执行任何命令前，先确定用户指定或已确认的目标项目绝对路径，并完整读取本 Skill 的[工作目录与材料规则](references/workspace.md)。后续显式设置 workdir/cwd；目标不明先询问，权限不足不转写会话目录。参考缺失时在已确认目标项目用对应宿主的 `sdd init` 刷新；仍不可用则说明阻塞，不从其他项目借用。

随后完整读取本 Skill 的 [AI-DLC 协作参考](references/collaboration.md)。协作参考不是新 Skill 或权限来源；遵循 Core 下发的行动、结果 Schema 和用户授权。

计划和任务文档只由目标项目 CLI 接收结果后生成，不手写 `.sdd/` 或另存多份计划；此阶段不修改业务文件。

先运行 `sdd status --json`；多任务且未指定时询问。运行 `sdd plan --change <id> --json`。每个任务必须是可独立验收的纵向切片，在内部 steps 中包含测试、实现和验证；不得把 RED、GREEN、REFACTOR、VERIFY 拆成独立任务。精确声明文件范围、依赖、验收和命令，按计划 resultSchema 4.0.0 填写 `collaboration`（拓扑、lead、贡献和限制），生成完整 JSON 后回传。不得修改业务文件。

按返回的完整 resultSchema 构造任务；command 只填写程序名，参数逐项放入 args，testSeam 填允许范围内的具体测试文件路径。forbiddenFiles 无额外限制时可为空数组。用户要求完整实现时继续 sdd-build；只要求计划时在计划完成后结束。

由 lead 整合 product、architect 和必要 support 的贡献后生成计划；普通 support 同时最多两个，不能把角色或拓扑拆成额外流程任务。

对用户简洁汇报业务结果、当前进度、验证和阻塞；内部标识、结果 JSON 和完整命令输出留给 Agent 处理。需要选择任务时展示需求标题与阶段，让用户按标题选择。
