# sdd-harness

> 面向 **Claude Code** 和 **Codex** 的插件式 Spec-Driven Development（SDD）工作流框架。

`sdd-harness` 把一句粗略的软件需求，转化为**可执行、可验证、可追踪**的开发流程，帮助 AI 编码工具按照稳定的工程步骤完成开发任务，而不是拿到需求就直接写代码。

第一版交付形态是**插件包 + 共享执行核心**，不发布独立 CLI。文档中的 `sdd init`、`sdd build` 等写法表示统一命令契约，在两个宿主中的触发方式不同：

| 宿主        | 触发方式      | 示例                      |
| ----------- | ------------- | ------------------------- |
| Claude Code | slash command | `/sdd.init`、`/sdd.build` |
| Codex       | 项目指令      | `sdd init`、`sdd build`   |

---

## 目录

- [解决什么问题](#解决什么问题)
- [核心特性](#核心特性)
- [工作流程](#工作流程)
- [安装与导入](#安装与导入)
  - [方式一：一键安装到 Claude Code](#方式一一键安装到-claude-code)
  - [方式二：一键安装到 Codex](#方式二一键安装到-codex)
  - [方式三：手工导入到 Claude Code](#方式三手工导入到-claude-code)
  - [方式四：手工导入到 Codex](#方式四手工导入到-codex)
  - [从源码构建后导入](#从源码构建后导入)
- [快速开始](#快速开始)
- [命令说明](#命令说明)
- [生成的制品](#生成的制品)
- [推荐使用方式](#推荐使用方式)
- [常见问题](#常见问题)
- [License](#license)

---

## 解决什么问题

在日常使用 Claude Code 或 Codex 时，AI 很容易直接根据一句需求开始写代码，导致：

- 需求没有澄清清楚
- 修改范围不可控
- 缺少设计和任务拆解
- 测试和验收不完整
- 代码审查依赖人工兜底
- 变更过程不可追踪

`sdd-harness` 通过强制的阶段化工作流和状态机来约束 AI 的行为，让每一次需求变更都经过澄清、设计、拆解、实现、验证、审查和归档，全过程留痕。

**适用场景**

- 使用 Claude Code 或 Codex 开发企业项目，希望编码过程更可控
- 希望 AI 在写代码前先做需求澄清和方案设计
- 希望减少无关修改、过度设计和低质量实现
- 希望每次需求变更都有文档记录，可验证、可审查、可归档

---

## 核心特性

### 1. 代码库感知

初始化项目时建立当前项目的代码库上下文（优先使用 `codebase-memory-mcp`，不可用时自动降级为受限文件扫描），让后续需求分析、方案设计和任务拆解基于真实代码结构进行。

### 2. 需求澄清

输入粗略需求后不会立即写代码，而是先进行需求分析，并自动提出需要确认的问题。存在未澄清问题时流程会停在 `CLARIFYING` 状态。

### 3. 阶段化开发

一次需求会被拆成多个清晰阶段，每个阶段都有明确目标和输出：

```text
new → design → plan → build → verify → review → archive
```

### 4. 自动编排

`auto` 命令是阶段编排器，从当前状态顺序推进，每次只调用一个单阶段命令，不绕过任何阶段自身的检查。遇到阻塞会暂停并提示下一步。

### 5. 可追踪制品与安全边界

- 每次需求变更都会生成对应的文档和记录，统一存放在项目的 `.sdd/` 目录。
- 状态文件采用原子写入 + 备份恢复，绝不猜测状态。
- 内置路径穿越防护、文件范围校验、只读 Git / 测试命令白名单；仓库内容与 MCP 输出**只作为数据**，不会被当作指令执行。

---

## 工作流程

```text
初始化项目 (init)
      ↓
创建 / 澄清需求 (new)
      ↓
生成设计方案 (design)
      ↓
拆解开发任务 (plan)
      ↓
实现代码 (build)
      ↓
验证功能 (verify)
      ↓
审查代码 (review)
      ↓
归档记录 (archive)
```

对应的状态机主路径：

```text
NOT_INITIALIZED → INDEX_READY → SPEC_READY → DESIGN_READY → PLAN_READY
                → BUILD_READY → VERIFY_READY → REVIEW_READY → ARCHIVED
```

---

## 安装与导入

### 前置要求

- Node.js **20 及以上**版本（macOS 或 Windows）
- Claude Code 或 Codex 宿主环境
- 可选：`codebase-memory-mcp v0.8.1`（MCP 不可用时自动降级为受限文件扫描）

> sdd-harness 是**插件**而非独立 CLI：所有命令都通过宿主环境（Claude Code / Codex）触发，宿主在加载插件时会创建对应 Adapter 并注入 `TaskExecutor` 与可选的 `McpTransport`。

如果你在自定义宿主或测试环境中直接使用插件包，可以显式创建适配器并注入运行时依赖：

```ts
import { CodexAdapter } from "@sdd-harness/codex-plugin";

const adapter = new CodexAdapter({
  taskExecutor,
  mcpTransport, // 可选
});
```

Claude Code 插件包同理：

```ts
import { ClaudeCodeAdapter } from "@sdd-harness/claude-code-plugin";

const adapter = new ClaudeCodeAdapter({
  taskExecutor,
  mcpTransport, // 可选
});
```

---

### 方式一：一键安装到 Claude Code

在当前仓库根目录执行：

```bash
node scripts/install-claude.mjs
```

这个脚本会先检查当前源码仓库是否可安装：

- 两个插件 manifest 是否存在且版本一致
- `dist/` 构建产物是否存在
- 根目录 `.claude-plugin/marketplace.json` 是否可用

脚本不会直接写入 Claude Code 的宿主内部安装状态，而是把最后一步宿主内安装/重载命令打印出来。也就是说，它会完成本地准备，但仍需你在 Claude Code 会话中执行：

```text
/plugin marketplace add /你的/sdd-harness/绝对路径
/plugin install sdd-harness@sdd-harness
```

完成最后一步宿主内安装/重载后，可用以下命令验证：

```text
/sdd.init
/sdd.status
```

> 适用范围：仅支持把“当前这份源码版本”安装到 Claude Code，不负责 npm / release 包安装。

---

### 方式二：一键安装到 Codex

在当前仓库根目录执行：

```bash
node scripts/install-codex.mjs
```

这个脚本会自动完成以下动作：

- 校验当前仓库的 Codex 插件布局与构建产物
- 把 `packages/codex-plugin` 复制到 `~/.codex/plugins/sdd-harness`
- 创建或更新 `~/.agents/plugins/marketplace.json`
- 幂等覆盖已有的 `sdd-harness` 本地 marketplace 条目

安装完成后，重启 Codex，并在目标项目根目录验证：

```text
sdd init
sdd status
```

> 说明：该脚本会修改用户目录下的 `~/.codex/plugins/sdd-harness` 和 `~/.agents/plugins/marketplace.json`。如果你更新了当前仓库代码，重新执行一次脚本即可覆盖本地安装内容。

---

### 方式三：手工导入到 Claude Code

Claude Code 通过 **plugin marketplace** 机制加载插件。本仓库根目录已提供 `.claude-plugin/marketplace.json`，指向 `packages/claude-code-plugin`。

**1. 添加 marketplace**

在 Claude Code 会话中执行（任选其一）：

```text
# 从 GitHub 仓库添加
/plugin marketplace add liuyi-it/sdd-harness

# 或从本地克隆的目录添加
/plugin marketplace add /path/to/sdd-harness
```

**2. 安装插件**

```text
/plugin install sdd-harness@sdd-harness
```

或直接打开交互式面板选择安装：

```text
/plugin
```

**3. 验证**

安装后重启 / 重载会话，输入 `/sdd.` 应能看到补全出的 slash command：

```text
/sdd.init  /sdd.new  /sdd.design  /sdd.plan  /sdd.build
/sdd.verify  /sdd.review  /sdd.archive  /sdd.auto  /sdd.status
```

在目标项目根目录执行 `/sdd.init` 即可开始。

> 说明：插件命令定义在 `packages/claude-code-plugin/commands/sdd.*.md`，技能约束在 `packages/claude-code-plugin/skills/sdd-harness/SKILL.md`。

---

### 方式四：手工导入到 Codex

Codex 通过 **plugin marketplace** 加载插件。当前仓库里的 Codex 插件目录是 `packages/codex-plugin`，清单文件是 `packages/codex-plugin/.codex-plugin/plugin.json`。

推荐使用 **personal marketplace**，这样不会改动目标项目仓库本身。

**1. 准备本地插件目录**

```bash
git clone https://github.com/liuyi-it/sdd-harness.git
```

把 `packages/codex-plugin` 复制到 Codex 的本地插件目录，例如：

macOS:

```bash
mkdir -p ~/.codex/plugins
cp -R /path/to/sdd-harness/packages/codex-plugin ~/.codex/plugins/sdd-harness
```

Windows PowerShell:

```powershell
New-Item -ItemType Directory -Force "$HOME/.codex/plugins" | Out-Null
Copy-Item -Recurse "C:/path/to/sdd-harness/packages/codex-plugin" "$HOME/.codex/plugins/sdd-harness"
```

**2. 注册 marketplace**

在 `~/.agents/plugins/marketplace.json` 增加一个本地插件入口，`source.path` 指向刚才复制后的目录。

示例：

```json
{
  "name": "local-personal",
  "plugins": [
    {
      "name": "sdd-harness",
      "source": {
        "source": "local",
        "path": "./.codex/plugins/sdd-harness"
      },
      "policy": {
        "installation": "AVAILABLE",
        "authentication": "ON_INSTALL"
      },
      "category": "Productivity"
    }
  ]
}
```

> `source.path` 必须以 `./` 开头，并且相对 `~/.agents/plugins/marketplace.json` 所在目录解析。

**3. 验证**

重启 Codex 后，插件应出现在本地 marketplace 中。进入目标项目根目录执行：

```text
sdd init
sdd status
```

能返回项目状态即表示导入成功。

如果只是修改了插件内容，需要重新复制 `packages/codex-plugin` 到 `~/.codex/plugins/sdd-harness`，然后重启 Codex 让本地安装重新加载。

---

### 从源码构建后导入

若需要本地开发或改动 Core 后再导入，先在仓库根目录构建产物：

```bash
# 安装依赖（npm workspaces，需 Node ≥ 20）
npm install

# 编译所有包（生成各插件包的 dist/）
npm run build

# 可选：跑测试确认核心行为
npm test
```

构建完成后：

- **Claude Code**：按[方式一](#方式一在-claude-code-中导入推荐)用本地路径 `/plugin marketplace add /path/to/sdd-harness` 添加。
- **Codex**：按[方式二](#方式二在-codex-中导入)把 `packages/codex-plugin` 复制到本地插件目录，并更新 `~/.agents/plugins/marketplace.json`。

> 卸载（当前 MVP 为手工方式）分两层：
>
> 1. 清理项目内集成文件：`.sdd/`、`.claude/commands/sdd.*`、`.claude/skills/sdd-harness/`、`.codex/skills/sdd-harness/`；
> 2. 清理宿主侧插件安装：删除 Claude marketplace 安装项，或删除 Codex 本地插件目录 `~/.codex/plugins/sdd-harness` 及其 `~/.agents/plugins/marketplace.json` 条目。  
>    重复执行 `init` 会保留用户手改过的文件，仅补回缺失的生成文件。

---

## 快速开始

### 1. 初始化项目

在目标项目根目录执行，会建立代码库上下文并生成 SDD 工作目录：

```text
Claude Code: /sdd.init
Codex:       sdd init
```

### 2. 自动执行需求

一条命令跑完整流程（需求澄清 → 规格 → 设计 → 拆解 → 实现 → 验证 → 审查 → 归档）：

```text
Claude Code: /sdd.auto "实现订单取消功能"
Codex:       sdd auto "实现订单取消功能"
```

遇到阻塞（如需求需要澄清）时会暂停，并提示下一步操作。

### 3. 查看当前状态

```text
Claude Code: /sdd.status
Codex:       sdd status
```

示例输出：

```text
Project: order-service
Current Change: add-order-cancel
Current Phase: PLAN_READY
Index Status: READY

Next:
sdd build
```

### 手动控制每个阶段

不想全自动时，可逐阶段执行：

```text
Claude Code                       Codex
/sdd.new "实现订单取消功能"        sdd new "实现订单取消功能"
/sdd.design                       sdd design
/sdd.plan                         sdd plan
/sdd.build                        sdd build
/sdd.verify                       sdd verify
/sdd.review                       sdd review
/sdd.archive                      sdd archive
```

---

## 命令说明

| 命令      | 作用                                     | Claude Code        | Codex             |
| --------- | ---------------------------------------- | ------------------ | ----------------- |
| `init`    | 初始化项目并建立代码库上下文             | `/sdd.init`        | `sdd init`        |
| `auto`    | 自动执行完整 SDD 流程                    | `/sdd.auto "需求"` | `sdd auto "需求"` |
| `new`     | 创建需求变更，做需求分析、澄清与规格生成 | `/sdd.new "需求"`  | `sdd new "需求"`  |
| `design`  | 基于规格生成设计方案                     | `/sdd.design`      | `sdd design`      |
| `plan`    | 基于设计拆解开发任务                     | `/sdd.plan`        | `sdd plan`        |
| `build`   | 根据任务计划实现代码                     | `/sdd.build`       | `sdd build`       |
| `verify`  | 验证任务完成度与功能边界                 | `/sdd.verify`      | `sdd verify`      |
| `review`  | 审查代码质量、修改范围与实现合理性       | `/sdd.review`      | `sdd review`      |
| `archive` | 归档当前需求变更                         | `/sdd.archive`     | `sdd archive`     |
| `status`  | 查看当前 SDD 状态与下一步建议            | `/sdd.status`      | `sdd status`      |

**通用参数**：`--json`、`--non-interactive`、`--force`、`--timeout <seconds>`、`--change <id>`、`--verbose`、`--help`。`new` / `auto` 允许第一个非选项参数直接作为自然语言需求。

---

## 生成的制品

`sdd-harness` 会为每次需求变更生成对应记录，统一存放在项目的 `.sdd/` 目录：

- 需求说明与澄清问题
- 需求规格
- 设计方案
- 任务拆解与测试计划
- 验证报告
- 审查报告
- 归档报告

每个 Markdown 制品都配 `*.meta.json` 记录输入摘要与 SHA-256；重复运行相同输入会写 `*.candidate.md` 而非直接覆盖。

---

## 推荐使用方式

**首次接入项目**

```text
sdd init
```

**日常开发需求**

```text
sdd auto "你的需求描述"
```

**复杂需求或高风险变更**（逐阶段把控）

```text
sdd new "你的需求描述"
sdd design
sdd plan
sdd build
sdd verify
sdd review
sdd archive
```

---

## 常见问题

**Q：安装后 `/sdd.*` 命令不出现？**
确认已 `/plugin install sdd-harness@sdd-harness` 并重载会话；本地路径添加 marketplace 时需指向仓库根目录（含 `.claude-plugin/marketplace.json`）。

**Q：没有 `codebase-memory-mcp` 能用吗？**
可以。MCP 不可用时会自动降级为受限文件扫描（跳过 `.git`、`node_modules`、`dist` 等目录），功能可用但代码库上下文精度略低。

**Q：流程卡在 `CLARIFYING` / `PAUSED` / `FAILED`？**
这是设计行为——存在未澄清问题、被中断或阶段校验失败时会停下。执行 `sdd status` 查看 Core 给出的恢复命令。

---

## License

MIT
