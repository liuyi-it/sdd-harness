/** Agent 能力等级 — 任何 AI Coding Agent 接入 sdd-harness 需满足的能力要求 */
export enum AgentCapabilityLevel {
  /** 只读 Agent */
  READ_ONLY = 0,
  /** 可运行 CLI */
  CAN_RUN_CLI = 1,
  /** 可读写项目文件 */
  CAN_READ_WRITE = 2,
  /** 可运行测试命令 */
  CAN_RUN_TESTS = 3,
  /** 可返回 TaskExecutionResult — 完整 SDD build 最低要求 */
  CAN_RETURN_RESULT = 4,
  /** 支持 subagent / 并行任务 */
  CAN_SUBAGENT = 5,
}

/** 目标 Agent 能力预期 */
export const AGENT_CAPABILITY_MAP: Record<string, AgentCapabilityLevel> = {
  "Claude Code": AgentCapabilityLevel.CAN_SUBAGENT,
  Codex: AgentCapabilityLevel.CAN_SUBAGENT,
  OpenCode: AgentCapabilityLevel.CAN_RETURN_RESULT,
  "Kimi Code": AgentCapabilityLevel.CAN_RUN_TESTS,
  "GitHub Copilot CLI": AgentCapabilityLevel.CAN_RUN_TESTS,
};
