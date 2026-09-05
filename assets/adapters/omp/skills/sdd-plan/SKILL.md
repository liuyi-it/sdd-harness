---
name: sdd-plan
description: 把已批准的统一规格拆成可执行、可验证的纵向任务时使用。
---

# SDD Plan

先运行 status；多任务且未指定时询问。运行 `sdd plan --change <id> --json`。每个任务是完整纵向切片，测试、实现和验证放在内部 steps，不拆成 RED/GREEN/REFACTOR/VERIFY 四个任务；精确声明范围、依赖、验收和命令后回传 resultSchema JSON。

按返回的完整 resultSchema 构造任务；command 只填写程序名，参数逐项放入 args，testSeam 填允许范围内的具体测试文件路径。forbiddenFiles 无额外限制时可为空数组。用户要求完整实现时继续 sdd-build；只要求计划时在计划完成后结束。

对用户简洁汇报业务结果、当前进度、验证和阻塞；内部标识、结果 JSON 和完整命令输出留给 Agent 处理。需要选择任务时展示需求标题与阶段，让用户按标题选择。
