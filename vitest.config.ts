import { defineConfig } from "vitest/config";
import { fileURLToPath } from "node:url";

export default defineConfig({
  resolve: {
    alias: {
      "@sdd-harness/core": fileURLToPath(
        new URL("./packages/core/src/index.ts", import.meta.url),
      ),
    },
  },
  test: {
    // 端到端质量链会创建临时 Git 仓库并执行完整 RED/GREEN/REFACTOR/VERIFY；
    // 30 秒避免 Windows 文件系统或 CI 负载导致的非功能性超时。
    testTimeout: 30_000,
    coverage: {
      provider: "v8",
      reporter: ["text", "json-summary"],
    },
    include: ["packages/**/*.test.ts", "test/**/*.test.ts"],
  },
});
