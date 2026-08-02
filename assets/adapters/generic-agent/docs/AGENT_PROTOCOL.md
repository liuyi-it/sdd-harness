# Generic Agent Protocol

任何 AI Coding Agent 只需满足以下条件即可接入 sdd-harness：

1. 能运行 CLI 命令
2. 能读取文件
3. 能修改文件
4. 能运行测试命令
5. 能写 JSON 结果文件

## Agent 能力等级

| Level | 能力           | 说明                                                         |
| ----- | -------------- | ------------------------------------------------------------ |
| 0     | 只读 Agent     | 只能读取文件                                                 |
| 1     | 可运行 CLI     | 可以执行 `sdd` 命令                                          |
| 2     | 可读写项目文件 | 可以修改源代码                                               |
| 3     | 可运行测试命令 | 可以执行测试                                                 |
| 4     | 可返回结果     | 可以生成 TaskExecutionResult JSON（完整 SDD build 最低要求） |
| 5     | 支持 subagent  | 可以并行执行多个子任务                                       |

## Agent Loop

```
while true:
  result = sdd auto --json

  if result.state in [ARCHIVED, CLARIFYING, FAILED, PAUSED, SECURITY_BLOCKED]:
    stop

  if result.actionRequired.type == AGENT_TASK_EXECUTION:
    execute task
    sdd build complete
```

## 默认限制

- maxLoopSteps: 8
- maxBuildTasksPerRun: 20
- maxClarifyingQuestionsPerRound: 5
