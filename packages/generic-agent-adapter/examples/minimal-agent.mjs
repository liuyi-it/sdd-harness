#!/usr/bin/env node
// 最小可运行 Agent 示例 — 展示完整的 auto loop
import { execSync } from "node:child_process";

function main() {
  const requirement = process.argv[2] || "实现订单取消功能";
  let step = 0;

  while (step < 8) {
    step++;
    const output = execSync(`sdd auto "${requirement}" --json`, {
      encoding: "utf-8",
    });
    const result = JSON.parse(output);

    // 终端状态 → 停止
    if (["ARCHIVED", "CLARIFYING", "FAILED", "PAUSED"].includes(result.state)) {
      console.log(`Terminal state: ${result.state}`);
      break;
    }

    // build task → 执行
    if (result.actionRequired?.type === "AGENT_TASK_EXECUTION") {
      const task = result.actionRequired;
      console.log(`Executing task: ${task.taskId}`);

      // 1. 读取 contextPack
      // 2. 修改 allowedFiles
      // 3. 运行 verification
      // 4. 写 TaskExecutionResult
      // 5. 提交 sdd build complete
    }
  }
}

main();
