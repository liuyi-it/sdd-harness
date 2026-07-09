# 移除候选文件机制，改为就地合并 — 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 彻底移除 `.candidate.md` 候选文件机制，受管文件直接覆盖，制品文件在已有内容上就地合并。

**Architecture:** 分三阶段推进 — Phase 1 清理 init 受管文件的候选逻辑（无外部依赖），Phase 2 扩展引擎接口增加 existing\* 参数，Phase 3 重构三个制品命令（design/new/plan）实现内联幂等判断 + 就地合并。最后统一移除 ArtifactWriter 中不再使用的候选方法并更新所有测试。

**Tech Stack:** TypeScript, Node.js fs/promises, Vitest

## 全局约束

- 指令文件（CLAUDE.md / AGENTS.md）的行级去重追加策略不变。
- `ArtifactWriter.write()` 的写 `.meta.json` 行为不变。
- `inputHash` 幂等判断机制保留（仅从 `writeOrCandidate` 中提取到命令层）。
- `--force` 参数行为保持不变（直接覆盖，不走合并逻辑）。
- 所有新增接口参数均为可选（`?`），保持向后兼容。

---

### Task 1: 简化 project-installer.ts — 移除 writeManagedFile

**Files:**

- Modify: `packages/core/src/install/project-installer.ts`

**Interfaces:**

- Consumes: `ArtifactWriter` from `../artifacts/artifact-writer.js`
- Produces: `installProjectIntegration(): Promise<void>`（原返回 `ProjectIntegrationResult`）

- [ ] **Step 1: 删除 `writeManagedFile` 函数（第 162-185 行）**

- [ ] **Step 2: 删除 `ProjectIntegrationResult` 接口（第 9-11 行）**

```typescript
// 删除以下代码：
export interface ProjectIntegrationResult {
  candidateFiles: string[];
}
```

- [ ] **Step 3: 修改 `installProjectIntegration` 签名和返回值**

```typescript
// 之前：
export async function installProjectIntegration(
  root: string,
  manifests: AdapterManifest[],
  options: { force?: boolean } = {},
): Promise<ProjectIntegrationResult> {
  const results = await Promise.all([
    ...manifests.flatMap((manifest) => [
      installInstructions(...),
      installCommands(root, manifest, options),
      ...(manifest.skillsDir !== undefined && manifest.skillContent !== undefined
        ? [installSkill(root, manifest, options)] : []),
      ...(manifest.rules !== undefined
        ? manifest.rules.map((rule) => installRule(root, rule.file, rule.content, options))
        : []),
    ]),
    installSchemas(root, options),
  ]);
  return { candidateFiles: results.flatMap((result) => result.candidateFiles) };
}

// 之后：
export async function installProjectIntegration(
  root: string,
  manifests: AdapterManifest[],
  _options: { force?: boolean } = {},
): Promise<void> {
  await Promise.all([
    ...manifests.flatMap((manifest) => [
      installInstructions(...),
      installCommands(root, manifest),
      ...(manifest.skillsDir !== undefined && manifest.skillContent !== undefined
        ? [installSkill(root, manifest)] : []),
      ...(manifest.rules !== undefined
        ? manifest.rules.map((rule) => installRule(root, rule.file, rule.content))
        : []),
    ]),
    installSchemas(root),
  ]);
}
```

- [ ] **Step 4: 修改 `installCommands` 使用 `ArtifactWriter.write()` 直接写入**

```typescript
// 之前：
async function installCommands(
  root: string,
  manifest: AdapterManifest,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const directory = join(root, manifest.commandsDir);
  await mkdir(directory, { recursive: true });
  const results = await Promise.all(
    COMMANDS.map((command) =>
      writeManagedFile(
        join(directory, `sdd.${command}.md`),
        manifest.commandTemplate.replaceAll("{command}", command),
        options,
      ),
    ),
  );
  return { candidateFiles: results.flatMap((result) => result.candidateFiles) };
}

// 之后：
async function installCommands(
  root: string,
  manifest: AdapterManifest,
): Promise<void> {
  const directory = join(root, manifest.commandsDir);
  await mkdir(directory, { recursive: true });
  const writer = new ArtifactWriter();
  const inputs = managedInputs(manifest.commandTemplate);
  await Promise.all(
    COMMANDS.map((command) =>
      writer.write(
        join(directory, `sdd.${command}.md`),
        manifest.commandTemplate.replaceAll("{command}", command),
        inputs,
      ),
    ),
  );
}
```

- [ ] **Step 5: 修改 `installSkill` 使用 `ArtifactWriter.write()` 直接写入**

```typescript
// 之前：
async function installSkill(
  root: string,
  manifest: AdapterManifest,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const skillPath = join(root, manifest.skillsDir!, "SKILL.md");
  await mkdir(join(skillPath, ".."), { recursive: true });
  return writeManagedFile(skillPath, manifest.skillContent!, options);
}

// 之后：
async function installSkill(
  root: string,
  manifest: AdapterManifest,
): Promise<void> {
  const skillPath = join(root, manifest.skillsDir!, "SKILL.md");
  await mkdir(join(skillPath, ".."), { recursive: true });
  await new ArtifactWriter().write(
    skillPath,
    manifest.skillContent!,
    managedInputs(manifest.skillContent!),
  );
}
```

- [ ] **Step 6: 修改 `installRule` 使用 `ArtifactWriter.write()` 直接写入**

```typescript
// 之前：
async function installRule(
  root: string,
  ruleFile: string,
  content: string,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const rulePath = join(root, ruleFile);
  await mkdir(join(rulePath, ".."), { recursive: true });
  return writeManagedFile(rulePath, content, options);
}

// 之后：
async function installRule(
  root: string,
  ruleFile: string,
  content: string,
): Promise<void> {
  const rulePath = join(root, ruleFile);
  await mkdir(join(rulePath, ".."), { recursive: true });
  await new ArtifactWriter().write(rulePath, content, managedInputs(content));
}
```

- [ ] **Step 7: 修改 `installSchemas` 使用 `ArtifactWriter.write()` 直接写入**

```typescript
// 之前：
async function installSchemas(
  root: string,
  options: { force?: boolean },
): Promise<ProjectIntegrationResult> {
  const directory = join(root, ".sdd", "schemas");
  await mkdir(directory, { recursive: true });
  const results = await Promise.all(
    Object.entries(CANONICAL_SCHEMAS).map(async ([name, content]) =>
      writeManagedFile(join(directory, name), content, options),
    ),
  );
  return { candidateFiles: results.flatMap((result) => result.candidateFiles) };
}

// 之后：
async function installSchemas(root: string): Promise<void> {
  const directory = join(root, ".sdd", "schemas");
  await mkdir(directory, { recursive: true });
  const writer = new ArtifactWriter();
  await Promise.all(
    Object.entries(CANONICAL_SCHEMAS).map(async ([name, content]) =>
      writer.write(join(directory, name), content, managedInputs(content)),
    ),
  );
}
```

- [ ] **Step 8: 移除 `managedInputs` 和 `ensureMetadata`（不再被调用，但 `managedInputs` 被 `installInstructions` 使用，需保留）**

检查后保留 `managedInputs` 和 `ensureMetadata`（`installInstructions` 第 64、77、82、91 行仍在使用）。

- [ ] **Step 9: 不再使用的 `readFile` import 检查**

`installCommands` 不再需要 `readFile`（之前只在 `writeManagedFile` 中使用），但从顶部 import 移除时需确保没有其他函数使用。检查后 `readFile` 仍被 `installInstructions` 使用，保留。

- [ ] **Step 10: 运行类型检查和构建**

```bash
npm run typecheck
npm run build
```

- [ ] **Step 11: 提交**

```bash
git add packages/core/src/install/project-installer.ts
git commit -m "重构: project-installer 移除候选文件逻辑，改为直接写入"
```

---

### Task 2: 简化 loop-store.ts

**Files:**

- Modify: `packages/core/src/loop/loop-store.ts`

**Interfaces:**

- Consumes: `ArtifactWriter` from `../artifacts/artifact-writer.js`
- Produces: `writeSpec(): Promise<void>`（原返回 `"written" | "unchanged" | "candidate"`）

- [ ] **Step 1: 修改 `writeSpec` 方法**

```typescript
// 之前：
async writeSpec(
  spec: LoopSpec,
  options: { force?: boolean } = {},
): Promise<"written" | "unchanged" | "candidate"> {
  await mkdir(this.runsDirectory, { recursive: true });
  return new ArtifactWriter().writeOrCandidate(
    this.specPath,
    JSON.stringify(spec, null, 2),
    spec,
    options,
  );
}

// 之后：
async writeSpec(spec: LoopSpec): Promise<void> {
  await mkdir(this.runsDirectory, { recursive: true });
  await new ArtifactWriter().write(
    this.specPath,
    JSON.stringify(spec, null, 2),
    spec,
  );
}
```

- [ ] **Step 2: 运行类型检查和构建**

```bash
npm run typecheck
npm run build
```

- [ ] **Step 3: 提交**

```bash
git add packages/core/src/loop/loop-store.ts
git commit -m "重构: loop-store.writeSpec 改为直接写入，移除候选逻辑"
```

---

### Task 3: 简化 init.ts — 受管文件直接覆盖 + 移除候选警告

**Files:**

- Modify: `packages/core/src/commands/init.ts`

**Interfaces:**

- Consumes: `installProjectIntegration(): Promise<void>`, `loopStore.writeSpec()` 不再返回状态值
- Produces: `runInit()` 返回结果中的 warnings 不再包含候选文件警告

- [ ] **Step 1: config.yml 改用 `ArtifactWriter.write()` 直接覆盖**

第 122-130 行，将 `writeIfMissing` 替换为 `writer.write`：

```typescript
// 之前：
await writeIfMissing(
  join(sddRoot, "config.yml"),
  stringify(
    defaultConfig(
      root,
      manifests.map((m) => m.agent),
    ),
  ),
);

// 之后：
await writer.write(
  join(sddRoot, "config.yml"),
  stringify(
    defaultConfig(
      root,
      manifests.map((m) => m.agent),
    ),
  ),
  { generatedBy: "sdd-harness", purpose: "config" },
);
```

注意：`writer` 变量在第 150 行才声明，需要上移到此处。将第 150 行的 `const writer = new ArtifactWriter();` 移到第 119 行（`const selectedAgents = ...` 之后）。

- [ ] **Step 2: 修改 `installProjectIntegration` 调用，不接收返回值**

第 133-135 行：

```typescript
// 之前：
const integration = await installProjectIntegration(root, manifests, {
  force: args?.force === true,
});

// 之后：
await installProjectIntegration(root, manifests, {
  force: args?.force === true,
});
```

- [ ] **Step 3: 修改 `loopStore.writeSpec` 调用，不接收返回值**

第 186-188 行：

```typescript
// 之前：
const loopOutcome = await loopStore.writeSpec(createDefaultLoopSpec(), {
  force: args?.force === true,
});

// 之后：
await loopStore.writeSpec(createDefaultLoopSpec());
```

- [ ] **Step 4: 简化 `buildWarnings` 函数签名和实现**

第 470-495 行：

```typescript
// 之前：
function buildWarnings(
  index: { degraded: boolean; reason?: string | null },
  candidateFiles: string[],
  configWarnings: string[],
  loopOutcome?: "written" | "unchanged" | "candidate",
): { warnings?: string[] } {
  const warnings: string[] = [];
  if (index.degraded) {
    warnings.push(
      "降级模式：codebase-memory-mcp 当前不可用，已切换为受限文件扫描",
    );
    warnings.push(
      `安装建议：请先安装并配置 codebase-memory-mcp，官方项目地址：${PINNED_DEPENDENCIES.codebaseMemoryMcp.repository}`,
    );
  }
  if (candidateFiles.length > 0) {
    warnings.push(
      `检测到人工修改，已生成候选文件供人工合并：${candidateFiles.join(", ")}`,
    );
  }
  if (loopOutcome === "candidate") {
    warnings.push("检测到人工修改的 loop spec，已生成候选文件供人工合并");
  }
  warnings.push(...configWarnings);
  return warnings.length === 0 ? {} : { warnings };
}

// 之后：
function buildWarnings(
  index: { degraded: boolean; reason?: string | null },
  configWarnings: string[],
): { warnings?: string[] } {
  const warnings: string[] = [];
  if (index.degraded) {
    warnings.push(
      "降级模式：codebase-memory-mcp 当前不可用，已切换为受限文件扫描",
    );
    warnings.push(
      `安装建议：请先安装并配置 codebase-memory-mcp，官方项目地址：${PINNED_DEPENDENCIES.codebaseMemoryMcp.repository}`,
    );
  }
  warnings.push(...configWarnings);
  return warnings.length === 0 ? {} : { warnings };
}
```

- [ ] **Step 5: 更新两处 `buildWarnings` 调用**

第 253-258 行（正常完成路径）：

```typescript
// 之前：
...buildWarnings(
  index,
  integration.candidateFiles,
  configWarnings,
  loopOutcome,
),

// 之后：
...buildWarnings(index, configWarnings),
```

第 218-220 行（空项目暂停路径）：

```typescript
// 之前：
...(loopOutcome === "candidate"
  ? ["检测到人工修改的 loop spec，已生成候选文件供人工合并"]
  : []),

// 之后：删除这三行（loopOutcome 变量已不存在）
```

- [ ] **Step 6: 移除不再使用的 `writeIfMissing` 函数（第 360-362 行）及 `access`、`copyFile`、`appendFile` import（如果不再使用）**

检查：`copyFile` 在 `migrateConfigIfNeeded` (第 591 行) 仍使用；`appendFile` 在 `migrateConfigIfNeeded` (第 594 行) 仍使用；`access` 在 `exists`（第 603 行）仍使用。仅移除 `writeIfMissing` 函数。

- [ ] **Step 7: 运行类型检查和构建**

```bash
npm run typecheck
npm run build
```

- [ ] **Step 8: 提交**

```bash
git add packages/core/src/commands/init.ts
git commit -m "重构: init 命令移除候选文件逻辑，受管文件直接覆盖"
```

---

### Task 4: TddEngine.generateDesign 增加 existingDesign 参数

**Files:**

- Modify: `packages/core/src/engines/tdd/tdd-engine.ts`

**Interfaces:**

- Consumes: 无
- Produces: `generateDesign(input: DesignInput): MaybePromise<string>`，`DesignInput` 增加 `existingDesign?: string`

- [ ] **Step 1: 在 `DesignInput` 接口增加 `existingDesign` 字段**

文件顶部（当前第 9-15 行），`DesignInput` 增加可选字段：

```typescript
interface DesignInput {
  spec: string;
  impact: string;
  codebaseSummary: string;
  packageStructure: string;
  architecture: string;
  existingDesign?: string;
}
```

- [ ] **Step 2: 在 `generateDesign` 方法中，当 `existingDesign` 存在时调整生成逻辑**

第 22 行起的 `generateDesign` 方法，在构建 prompt 时，如果 `existingDesign` 有值，追加以下指令：

```typescript
generateDesign(input: DesignInput): MaybePromise<string> {
  // ... 现有 prompt 构建逻辑 ...
  if (input.existingDesign !== undefined) {
    prompt += `

## 已有设计文档

以下是上次生成的设计文档。用户可能已在其上做了修改。请在已有内容基础上更新设计，遵循以下规则：
1. 保留用户新增或修改的内容（手动添加的需求分析、约束、取舍说明等）。
2. 仅因 spec/impact 变更而需要调整的部分才更新。
3. 输出完整的设计文档，不要输出 diff 或标记变更。`;
    prompt += `\n\n${input.existingDesign}`;
  }
  // ... 返回生成结果 ...
}
```

- [ ] **Step 3: 运行类型检查和构建**

```bash
npm run typecheck
npm run build
```

- [ ] **Step 4: 提交**

```bash
git add packages/core/src/engines/tdd/tdd-engine.ts
git commit -m "功能: TddEngine.generateDesign 支持传入已有设计文档进行就地编辑"
```

---

### Task 5: SpecEngine.generate 增加 existingSpec 参数

**Files:**

- Modify: `packages/core/src/engines/spec/spec-engine.ts`

**Interfaces:**

- Consumes: 无
- Produces: `generate(input: GenerateSpecInput): MaybePromise<SpecArtifacts>`，`GenerateSpecInput` 增加 `existingSpec?`

- [ ] **Step 1: 在 `GenerateSpecInput` 接口增加 `existingSpec` 字段**

```typescript
interface GenerateSpecInput {
  requirement: string;
  codebaseSummary: string;
  answers?: Record<string, string>;
  existingSpec?: {
    spec: string;
    delta: string;
    model: SpecDocument;
  };
}
```

- [ ] **Step 2: 在 `generate` 方法中，当 `existingSpec` 存在时调整生成逻辑**

```typescript
generate(input: GenerateSpecInput): MaybePromise<SpecArtifacts> {
  // ... 现有 prompt 构建逻辑 ...
  if (input.existingSpec !== undefined) {
    prompt += `

## 已有规格制品

以下是上次生成的规格文件。用户可能已在其上做了修改。请在已有内容基础上更新，遵循以下规则：
1. 保留用户新增或修改的内容。
2. 仅因需求变更而需要调整的部分才更新。
3. 输出完整的规格文件。`;
    prompt += `\n\n### spec.md\n${input.existingSpec.spec}`;
    prompt += `\n\n### spec.delta.md\n${input.existingSpec.delta}`;
    prompt += `\n\n### spec.model.json\n${JSON.stringify(input.existingSpec.model, null, 2)}`;
  }
  // ... 返回生成结果 ...
}
```

- [ ] **Step 3: 运行类型检查和构建**

```bash
npm run typecheck
npm run build
```

- [ ] **Step 4: 提交**

```bash
git add packages/core/src/engines/spec/spec-engine.ts
git commit -m "功能: SpecEngine.generate 支持传入已有规格制品进行就地编辑"
```

---

### Task 6: TddEngine.generatePlan 增加 existingPlan 参数

**Files:**

- Modify: `packages/core/src/engines/superpowers/protocol.ts`

**Interfaces:**

- Consumes: 无
- Produces: `PlanningInput` 增加 `existingPlan?`

- [ ] **Step 1: 在 `PlanningInput` 接口增加 `existingPlan` 字段**

```typescript
interface PlanningInput {
  spec: string;
  design: string;
  impact: string;
  codebaseSummary: string;
  existingPlan?: {
    tasksMarkdown: string;
    testPlan: string;
    context: string;
  };
}
```

- [ ] **Step 2: 在 `TddEngine.generatePlan` 中使用 `existingPlan`**

`packages/core/src/engines/tdd/tdd-engine.ts` 的 `generatePlan` 方法：

```typescript
generatePlan(input: PlanningInput): MaybePromise<PlanArtifacts> {
  // ... 现有 prompt 构建逻辑 ...
  if (input.existingPlan !== undefined) {
    prompt += `

## 已有计划制品

以下是上次生成的计划文件。用户可能已在其上做了修改。请在已有内容基础上更新，遵循以下规则：
1. 保留用户新增、删除或重排的任务。
2. 保留用户修改的测试计划和上下文。
3. 仅因设计变更而需要调整的部分才更新。
4. 输出完整的计划文件。`;
    prompt += `\n\n### tasks.md\n${input.existingPlan.tasksMarkdown}`;
    prompt += `\n\n### test-plan.md\n${input.existingPlan.testPlan}`;
    prompt += `\n\n### context.md\n${input.existingPlan.context}`;
  }
  // ... 返回生成结果 ...
}
```

- [ ] **Step 3: 运行类型检查和构建**

```bash
npm run typecheck
npm run build
```

- [ ] **Step 4: 提交**

```bash
git add packages/core/src/engines/superpowers/protocol.ts packages/core/src/engines/tdd/tdd-engine.ts
git commit -m "功能: TddEngine.generatePlan 支持传入已有计划制品进行就地编辑"
```

---

### Task 7: design.ts — 实现就地合并

**Files:**

- Modify: `packages/core/src/commands/design.ts`

**Interfaces:**

- Consumes: `ArtifactWriter.write()`, `artifactInputHash()`, `TddEngine.generateDesign({ ...input, existingDesign? })`
- Produces: `runDesign()` — 移除候选分支，增加就地合并路径

- [ ] **Step 1: 导入 `artifactInputHash`**

第 5 行：

```typescript
// 之前：
import { ArtifactWriter } from "../artifacts/artifact-writer.js";

// 之后：
import {
  ArtifactWriter,
  artifactInputHash,
} from "../artifacts/artifact-writer.js";
```

- [ ] **Step 2: 替换 `writeOrCandidate` 逻辑为内联幂等判断 + 就地合并**

第 80-113 行，完整替换：

```typescript
// 之前（第 80-113 行）：
const writer = new ArtifactWriter();
const outcome = await writer.writeOrCandidate(
  join(change, "design.md"),
  await withTimeout(
    Promise.resolve(engine.generateDesign(input)),
    timeoutMilliseconds(args),
    "sdd design",
    signal,
  ),
  input,
  { force: args?.force === true },
);
if (outcome === "unchanged") {
  return {
    ok: true,
    state: "DESIGN_READY",
    exitCode: 0,
    changeId,
    next: "sdd plan",
    data: { alreadyReady: true },
  };
}
if (outcome === "candidate") {
  return {
    ok: true,
    state: "DESIGN_READY",
    exitCode: 0,
    changeId,
    next: "sdd plan",
    warnings: ["design 输入已变化；已生成 design.md.candidate.md 供人工合并"],
  };
}

// 之后：
const writer = new ArtifactWriter();
const designPath = join(change, "design.md");
const inputHash = artifactInputHash(input);
const force = args?.force === true;
let existingDesign: string | undefined;
let unchanged = false;

try {
  const metadata = JSON.parse(
    await readFile(`${designPath}.meta.json`, "utf8"),
  ) as { inputHash: string };
  if (metadata.inputHash === inputHash) {
    unchanged = true;
  } else {
    existingDesign = await readFile(designPath, "utf8");
  }
} catch {
  // 文件不存在 → 正常生成
}

if (unchanged) {
  return {
    ok: true,
    state: "DESIGN_READY",
    exitCode: 0,
    changeId,
    next: "sdd plan",
    data: { alreadyReady: true },
  };
}

if (!force && existingDesign !== undefined) {
  input.existingDesign = existingDesign;
}

const designContent = await withTimeout(
  Promise.resolve(engine.generateDesign(input)),
  timeoutMilliseconds(args),
  "sdd design",
  signal,
);
await writer.write(designPath, designContent, input);
```

- [ ] **Step 3: 需新增 `readFile` import**

```typescript
// 第 1 行，已有的 import 中确保包含 readFile：
import { readFile } from "node:fs/promises";
```

（已存在，无需改动）

- [ ] **Step 4: 运行类型检查和构建**

```bash
npm run typecheck
npm run build
```

- [ ] **Step 5: 提交**

```bash
git add packages/core/src/commands/design.ts
git commit -m "重构: sdd design 移除候选文件机制，改为就地合并已有设计文档"
```

---

### Task 8: new.ts — 实现就地合并

**Files:**

- Modify: `packages/core/src/commands/new.ts`

**Interfaces:**

- Consumes: `ArtifactWriter.write()`, `artifactInputHash()`, `SpecEngine.generate({ ...input, existingSpec? })`
- Produces: `runNew()` — 移除候选分支，增加就地合并路径

- [ ] **Step 1: 导入 `artifactInputHash`**

```typescript
// 之前：
import { ArtifactWriter } from "../artifacts/artifact-writer.js";

// 之后：
import {
  ArtifactWriter,
  artifactInputHash,
} from "../artifacts/artifact-writer.js";
```

- [ ] **Step 2: 在 engine.generate() 调用前读取已有规格并注入 existingSpec**

第 239 行（`if (unansweredBlockers.length > 0)` 检查块结束）之后、第 241 行（第二次 `engine.generate()`）之前，插入 `existingSpec` 读取逻辑：

```typescript
// 在 "if (unansweredBlockers.length > 0) { ... }" 块之后，第二次 engine.generate 之前插入：

const force = args.force === true;
let existingSpec: { spec: string; delta: string; model: unknown } | undefined;
if (!force) {
  let sameInput = false;
  try {
    const metaPath = `${join(changeDirectory, "spec.md")}.meta.json`;
    const metadata = JSON.parse(await readFile(metaPath, "utf8")) as {
      inputHash: string;
    };
    if (metadata.inputHash === artifactInputHash(requirementInputs)) {
      sameInput = true;
    }
  } catch {
    // 文件不存在或无 meta.json
  }
  if (!sameInput) {
    try {
      existingSpec = {
        spec: await readFile(join(changeDirectory, "spec.md"), "utf8"),
        delta: await readFile(join(changeDirectory, "spec.delta.md"), "utf8"),
        model: JSON.parse(
          await readFile(join(changeDirectory, "spec.model.json"), "utf8"),
        ),
      };
    } catch {
      // 制品不存在或无法读取
    }
  }
  if (existingSpec !== undefined) {
    generationInput.existingSpec = existingSpec;
  }
}
```

- [ ] **Step 3: 替换 `writeGroupOrCandidates` 为 `writer.write()` 直接写入**

第 256-271 行的 `writeGroupOrCandidates` 替换为：

```typescript
// 之前（第 256-271 行）：
const structuredInputs = requirementInputs;
const structuredOutcome = await writer.writeGroupOrCandidates(
  [
    { path: join(changeDirectory, "spec.md"), content: artifacts.spec },
    { path: join(changeDirectory, "spec.delta.md"), content: artifacts.delta },
    {
      path: join(changeDirectory, "spec.model.json"),
      content: JSON.stringify(artifacts.model, null, 2),
    },
  ],
  structuredInputs,
  { force: args.force === true },
);

// 之后：
await Promise.all([
  writer.write(
    join(changeDirectory, "spec.md"),
    artifacts.spec,
    requirementInputs,
  ),
  writer.write(
    join(changeDirectory, "spec.delta.md"),
    artifacts.delta,
    requirementInputs,
  ),
  writer.write(
    join(changeDirectory, "spec.model.json"),
    JSON.stringify(artifacts.model, null, 2),
    requirementInputs,
  ),
]);
```

- [ ] **Step 3: 删除候选警告分支**

第 301-307 行，删除：

```typescript
// 删除：
...(structuredOutcome === "candidate"
  ? {
      warnings: [
        "检测到结构化规格制品的人工修改，已生成 candidate 文件供人工合并",
      ],
    }
  : {}),
```

同时将第 295 行的返回语句简化：

```typescript
// 之前：
return {
  ok: true,
  state: ready.currentPhase,
  exitCode: 0,
  changeId,
  next: "sdd design",
  ...(structuredOutcome === "candidate"
    ? ...
    : {}),
};

// 之后：
return {
  ok: true,
  state: ready.currentPhase,
  exitCode: 0,
  changeId,
  next: "sdd design",
};
```

- [ ] **Step 4: 运行类型检查和构建**

```bash
npm run typecheck
npm run build
```

- [ ] **Step 5: 提交**

```bash
git add packages/core/src/commands/new.ts
git commit -m "重构: sdd new 移除候选文件机制，改为就地合并已有规格制品"
```

---

### Task 9: plan.ts — 实现就地合并

**Files:**

- Modify: `packages/core/src/commands/plan.ts`

**Interfaces:**

- Consumes: `ArtifactWriter.write()`, `artifactInputHash()`, `TddEngine.generatePlan({ ...input, existingPlan? })`
- Produces: `runPlan()` — 移除候选分支，增加就地合并路径

- [ ] **Step 1: `artifactInputHash` 已在 plan.ts 中导入（第 7 行），无需改动**

- [ ] **Step 2: 替换三个 `writeOrCandidate` 为内联幂等判断 + 就地合并**

第 87-107 行，完整替换：

```typescript
// 之前（第 87-107 行）：
const writer = new ArtifactWriter();
const outcomes = await Promise.all([
  writer.writeOrCandidate(
    join(change, "tasks.md"),
    artifacts.tasksMarkdown,
    input,
    { force: args?.force === true },
  ),
  writer.writeOrCandidate(
    join(change, "test-plan.md"),
    artifacts.testPlan,
    input,
    { force: args?.force === true },
  ),
  writer.writeOrCandidate(
    join(change, "context.md"),
    artifacts.context,
    input,
    { force: args?.force === true },
  ),
]);
if (outcomes.every((outcome) => outcome === "unchanged")) {
  return {
    ok: true,
    state: "PLAN_READY",
    exitCode: 0,
    changeId,
    next: "sdd build",
    data: { alreadyReady: true },
  };
}
if (outcomes.some((outcome) => outcome === "candidate")) {
  return {
    ok: true,
    state: "PLAN_READY",
    exitCode: 0,
    changeId,
    next: "sdd build",
    warnings: ["plan 输入已变化；已生成候选制品供人工合并"],
  };
}

// 之后：
const writer = new ArtifactWriter();
const force = args?.force === true;
const inputHash = artifactInputHash(input);
let existingPlan:
  | { tasksMarkdown: string; testPlan: string; context: string }
  | undefined;
let unchanged = false;

try {
  const metaPath = `${join(change, "tasks.md")}.meta.json`;
  const metadata = JSON.parse(await readFile(metaPath, "utf8")) as {
    inputHash: string;
  };
  if (metadata.inputHash === inputHash) {
    try {
      // 确认三个文件都存在且未修改
      const tasksContent = await readFile(join(change, "tasks.md"), "utf8");
      const testPlanContent = await readFile(
        join(change, "test-plan.md"),
        "utf8",
      );
      const contextContent = await readFile(join(change, "context.md"), "utf8");
      existingPlan = {
        tasksMarkdown: tasksContent,
        testPlan: testPlanContent,
        context: contextContent,
      };
      unchanged = true;
    } catch {
      // 文件不完整，不算 unchanged
    }
  }
} catch {
  // 文件不存在
}

if (unchanged) {
  return {
    ok: true,
    state: "PLAN_READY",
    exitCode: 0,
    changeId,
    next: "sdd build",
    data: { alreadyReady: true },
  };
}

if (!force) {
  if (existingPlan === undefined) {
    try {
      existingPlan = {
        tasksMarkdown: await readFile(join(change, "tasks.md"), "utf8"),
        testPlan: await readFile(join(change, "test-plan.md"), "utf8"),
        context: await readFile(join(change, "context.md"), "utf8"),
      };
    } catch {
      // 文件不存在，不用合并
    }
  }
  if (existingPlan !== undefined) {
    input.existingPlan = existingPlan;
    const merged = await engine.generatePlan(input);
    artifacts.tasksMarkdown = merged.tasksMarkdown;
    artifacts.testPlan = merged.testPlan;
    artifacts.context = merged.context;
    artifacts.tasks = merged.tasks;
    artifacts.contextPacks = merged.contextPacks;
  }
}

await Promise.all([
  writer.write(join(change, "tasks.md"), artifacts.tasksMarkdown, input),
  writer.write(join(change, "test-plan.md"), artifacts.testPlan, input),
  writer.write(join(change, "context.md"), artifacts.context, input),
]);
```

- [ ] **Step 3: 删除候选警告分支（第 118-127 行）**

直接删除第 118-127 行的 `if (outcomes.some(...))` 块。

- [ ] **Step 4: 运行类型检查和构建**

```bash
npm run typecheck
npm run build
```

- [ ] **Step 5: 提交**

```bash
git add packages/core/src/commands/plan.ts
git commit -m "重构: sdd plan 移除候选文件机制，改为就地合并已有计划制品"
```

---

### Task 10: ArtifactWriter 移除 writeOrCandidate 和 writeGroupOrCandidates

**Files:**

- Modify: `packages/core/src/artifacts/artifact-writer.ts`

**Interfaces:**

- Consumes: 无
- Produces: 移除 `writeOrCandidate` 和 `writeGroupOrCandidates` 方法。`artifactInputHash` 保留（命令层仍在使用）。

- [ ] **Step 1: 删除 `writeOrCandidate` 方法（第 131-159 行）**

`artifactInputHash` 函数在方法中被调用但非唯一引用，检查后 `artifactInputHash` 被 `plan.ts` 和 `new.ts`（新代码）使用，保留导出。

- [ ] **Step 2: 删除 `writeGroupOrCandidates` 方法（第 161-230 行）**

- [ ] **Step 3: 检查 `isEnoent` 是否仍被使用**

`isEnoent` 被 `writeGroupOrCandidates` 和 `writeGroupAtomically` 使用。`writeGroupAtomically` 保留，所以 `isEnoent` 保留。

- [ ] **Step 4: 检查 `isArtifactMetadata` 是否仍被使用**

`isArtifactMetadata` 被 `isUnmodified`、`writeGroupOrCandidates` 使用。`isUnmodified` 保留，所以 `isArtifactMetadata` 保留。

- [ ] **Step 5: 运行类型检查和构建**

```bash
npm run typecheck
npm run build
```

- [ ] **Step 6: 提交**

```bash
git add packages/core/src/artifacts/artifact-writer.ts
git commit -m "重构: ArtifactWriter 移除 writeOrCandidate 和 writeGroupOrCandidates 方法"
```

---

### Task 11: 更新所有测试

**Files:**

- Modify: `packages/core/test/init-status.test.ts`
- Modify: `packages/core/test/init-agent-selection.test.ts`
- Modify: `packages/core/test/loop.test.ts`
- Modify: `packages/core/test/design-plan.test.ts`
- Modify: `packages/core/test/new.test.ts`
- Modify: `packages/core/test/artifact-writer.test.ts`

**Interfaces:**

- 所有测试需对齐新的行为：不再生成 `.candidate.md`，受管文件直接覆盖，制品文件就地合并。

- [ ] **Step 1: 删除 `artifact-writer.test.ts` 中的候选相关测试**

删除以下测试用例（第 56-168 行）：

- `keeps unchanged files idempotent and protects changed inputs with candidates` (第 56-76 行)
- `keeps a related artifact group consistent when one file was edited` (第 78-97 行)
- `repairs only missing primary files when existing group inputs match` (第 99-115 行)
- `writes a complete candidate group for %s inputs` (第 117-139 行)
- `protects existing primary files when metadata is %s` (第 142-168 行)

注意：第 17-54 行的 `组写发布阶段 rename 失败时完整恢复已有主制品` 测试的是 `writeGroupAtomically`，保留。

- [ ] **Step 2: 更新 `init-status.test.ts` 候选文件测试**

第 709-745 行，将候选文件生成测试改为验证直接覆盖：

```typescript
// 之前：验证生成 candidate 文件
// 之后：验证直接覆盖（受管文件内容被更新为最新版本）
```

第 747-783 行的 `--force` 测试保留（行为一致）。

- [ ] **Step 3: 更新 `init-agent-selection.test.ts` 移除 candidateFiles 断言**

移除所有 `result.candidateFiles` 相关的断言。`installProjectIntegration` 现在返回 `void`。

- [ ] **Step 4: 更新 `loop.test.ts` 候选测试**

第 30-61 行，`writes a default loop spec during init and protects manual edits with candidate` 测试改为验证直接写入。

- [ ] **Step 5: 更新 `design-plan.test.ts` 候选相关测试**

第 218-247 行：改为验证就地合并（已有设计文档内容被更新而不是生成 .candidate.md）。
第 249-276 行：改为验证就地合并已有计划制品。
第 278-305 行：`--force` 测试保留。

- [ ] **Step 6: 更新 `new.test.ts` 候选相关测试**

第 210-228 行：改为验证就地合并。
第 340-366 行：改为验证元数据更新。

- [ ] **Step 7: 运行完整测试套件**

```bash
npm test
```

预期：所有测试通过。

- [ ] **Step 8: 提交**

```bash
git add packages/core/test/
git commit -m "测试: 更新测试以对齐候选文件移除和就地合并行为"
```

---

### Task 12: 最终验证和清理

- [ ] **Step 1: 运行全量检查**

```bash
npm run format:check && npm run lint && npm run typecheck && npm test
```

- [ ] **Step 2: 确认无 `.candidate.md` 搜索残留**

```bash
grep -r "candidate" packages/core/src/ --include="*.ts" | grep -v node_modules | grep -v ".test.ts"
```

预期输出为空（源码中不再有 candidate 引用）。

- [ ] **Step 3: 提交**

```bash
git add -A
git commit -m "验证: 最终清理，确保候选文件逻辑完全移除"
```

---

## 任务依赖图

```
Phase 1 (可并行)
  Task 1 (project-installer) ──┐
  Task 2 (loop-store)       ──┤
  Task 3 (init.ts)          ──┘
                                │
Phase 2 (可并行)                │
  Task 4 (generateDesign)   ──┐ │
  Task 5 (SpecEngine)       ──┤ │
  Task 6 (generatePlan)     ──┘ │
                                │
Phase 3 (依赖 Phase 2)          │
  Task 7 (design.ts)        ──┐ │
  Task 8 (new.ts)           ──┤ │
  Task 9 (plan.ts)          ──┘ │
                                │
Phase 4                         │
  Task 10 (ArtifactWriter) ─────┘ (等所有调用方更新完)
  Task 11 (测试)            ─────  (等所有实现改动完成)
  Task 12 (最终验证)        ─────  (等测试通过)
```
