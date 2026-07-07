# 仓库协作指南

## 项目结构与模块划分

本仓库是一个基于 Node.js workspaces 的多包项目。核心流程在 `packages/core/src`，包括状态机、命令实现、安全校验和安装器；对应测试在 `packages/core/test`。CLI 入口在 `packages/cli`，提供 `sdd` / `sdd-harness` 命令。Agent Adapte 层分为 `packages/claude-code-adapter`、`packages/codex-adapter`、`packages/opencode-adapter`，分别提供命令/技能清单与规则文件。`packages/codebase-memory` 负责内置托管 codebase-memory-mcp 及降级处理。`packages/agent-protocol` 定义 Agent Task Protocol 类型与校验。`docs/` 存放架构、命令契约和安全说明，`fixtures/` 提供测试样例项目。

## Karpathy 风格执行规则

1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。
2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。
3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。
4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。

## 构建、测试与开发命令

- `npm install`：安装根项目与 workspaces 依赖。
- `npm run build`：执行 TypeScript 构建，产出各包编译结果。
- `npm run typecheck`：仅做类型检查，不生成额外产物。
- `npm run lint`：运行 ESLint，检查代码风格与常见错误。
- `npm run format:check`：检查 Prettier 格式是否一致。
- `npm run format`：自动格式化仓库文件。
- `npm test`：运行全部 Vitest 测试。

提交前至少运行 `npm run format:check && npm run lint && npm run typecheck && npm test`。

## 编码风格与命名约定

默认使用 TypeScript ESM、2 空格缩进，遵循现有文件风格。优先做小而集中的改动，不重写无关代码。文件名保持小写短横线或现有命名方式，如 `project-installer.ts`、`sdd.build.md`。新增注释以中文为主，重点解释约束、边界和原因，不写空洞注释。

## 测试要求

测试框架为 Vitest。单元测试使用 `*.test.ts` 命名，放在对应包的 `test` 目录。行为变更或缺陷修复必须补测试，优先覆盖 Core 命令契约、CLI 输出格式和 Adapter 契约一致性。

## 提交与 PR 规范

现有提交风格以简短前缀为主，例如 `docs: ...`、`i18n: ...`，也接受直接描述功能的提交标题。建议格式：`type: 简要说明`。PR 应说明变更目的、影响范围、验证命令与结果；若改动命令文案、README 或插件导入流程，附关键示例即可。

## 额外约束

不要提交密钥、凭据、生成产物或无关依赖变更。涉及 Adapter 行为时，同时检查 CLI 入口和 `packages/*-adapter` 自带命令/技能模板。

## 其他规则

1. 原始需求文档在 docs/需求文档.md;
2. git commit 中的内容，请使用中文说明；
3. 当前项目是**中文项目**，除给 AI 的 Prompt（skill、commands/\_.md 提示词）和代码中必要的英文（错误码 `E\__`、命令字面量 `sdd xxx`、schema 键、标识符）外，全项目中文化；
