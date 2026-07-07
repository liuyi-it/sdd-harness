/** Agent 行动要求 — build next 时返回，指导 Agent 执行构建任务 */
export interface AgentActionRequired {
  type: "AGENT_TASK_EXECUTION";
  taskId: string;
  changeId: string;
  /** Context Pack 文件路径 */
  contextPack: string;
  /** 允许修改的文件列表 */
  allowedFiles: string[];
  /** 期望新建的文件列表 */
  expectedNewFiles: string[];
  /** 禁止修改的文件列表 */
  forbiddenFiles: string[];
  /** 允许执行的验证命令 */
  verification: Array<{ command: string; args: string[] }>;
  /** Agent 任务结果写入路径 */
  resultFile: string;
  /** codebase 状态 */
  codebase: {
    provider: "codebase-memory-mcp" | "fallback-file-scan";
    degraded: boolean;
  };
}
