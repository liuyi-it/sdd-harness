# 设计文档：移除候选文件机制，改为就地合并

## 背景

当前 `sdd` 命令在已有制品存在且输入变化时，会生成 `.candidate.md` 文件供人工合并，而不是直接覆盖或就地编辑。这带来了两个问题：

1. **init 受管文件**（命令模板、skill、rule、schema、loop spec、config.yml）用户不会手动修改，生成候选文件没有意义。
2. **制品文件**（design.md、spec.md、tasks.md 等）用户在 sdd 生成后可能会编辑，但候选文件机制要求人工合并，体验割裂。

目标：彻底移除 `.candidate.md` 候选文件机制，改为在已有制品上智能合并。

## 核心原则

- **init 受管文件**：直接覆盖（用户不会手动修改这些文件）。
- **制品文件**：已有制品存在时，读取内容传给引擎，引擎在已有内容基础上生成合并版本，写回原文件。
- **幂等性保留**：通过 `inputHash` 比较判断输入是否变化，未变化时返回 "already ready"，不触发重复生成。
- **指令文件**（CLAUDE.md / AGENTS.md）的行级去重追加策略维持不变。

## 改动范围

### 1. `artifact-writer.ts` — 移除候选方法

删除 `writeOrCandidate()` 和 `writeGroupOrCandidates()` 两个方法。

- `write()` 方法保持不变（写入内容 + `.meta.json`，自动归一化末尾换行）。
- `artifactInputHash()` 和 `isUnmodified()` 保留，供命令层做幂等判断。

### 2. `project-installer.ts` — 简化受管文件安装

- **删除** `writeManagedFile()` 函数。
- **删除** `ProjectIntegrationResult` 类型（含 `candidateFiles` 字段）。
- `installProjectIntegration()` 返回类型改为 `Promise<void>`。
- `installCommands`、`installSkill`、`installRule`、`installSchemas` 直接调用 `new ArtifactWriter().write(path, content, managedInputs(content))`。
- `installInstructions` 保持不变（行级去重追加）。

### 3. `loop-store.ts` — 简化 loop spec 写入

- `writeSpec()` 改用 `new ArtifactWriter().write(this.specPath, content, spec)`。
- 返回类型从 `Promise<"written" | "unchanged" | "candidate">` 改为 `Promise<void>`。

### 4. `init.ts` — 简化 init 命令

- `config.yml` 写入：`writeIfMissing()` 改为 `new ArtifactWriter().write(path, content, configInputs)`，始终覆盖。
- `buildWarnings()` 移除 `candidateFiles` 和 `loopOutcome` 参数：
  - 删除候选文件警告逻辑（`candidateFiles.length > 0`）。
  - 删除 loop spec 候选警告逻辑（`loopOutcome === "candidate"`）。
- `installProjectIntegration()` 调用不再解构 `candidateFiles`（返回值已变为 void）。
- 空项目暂停分支中移除 loop spec 候选警告。

### 5. `design.ts` — 就地合并设计文档

**新流程：**

1. 读取 spec.md、impact.md、codebaseSummary 等，计算 `inputHash`。
2. 尝试读取已有 `design.md` 及 `.meta.json`。
3. 制品不存在 → `engine.generateDesign(input)` → `writer.write(design.md, content, input)`。
4. 制品存在 + `inputHash` 匹配 → 返回 "already ready"（幂等）。
5. 制品存在 + `inputHash` 不匹配 → 读取已有 design.md → `engine.generateDesign({ ...input, existingDesign })` → `writer.write(design.md, mergedContent, input)`。

**删除逻辑：**

- 删除 `outcome === "candidate"` 的警告分支。

### 6. `new.ts` — 就地合并规格制品

**新流程：**

1. 读取已有 spec.md、spec.delta.md、spec.model.json。
2. 如果存在 → `engine.generate({ ...input, existingSpec })` → `writer.write()` 回原文件。
3. 如果不存在 → 和现有逻辑一样（直接生成）。
4. 幂等判断通过 `.meta.json` 的 `inputHash` 比较。

**删除逻辑：**

- `writeGroupOrCandidates` 替换为内联的幂等判断 + 多次 `writer.write()` 调用。
- 删除 `structuredOutcome === "candidate"` 的候选警告分支。

### 7. `plan.ts` — 就地合并计划制品

**新流程：**

1. 读取已有 tasks.md、test-plan.md、context.md 及对应 `.meta.json`。
2. 全部存在且 `inputHash` 匹配 → 返回 "already ready"（幂等）。
3. 任一不匹配 → 读取已有制品 → `engine.generatePlan({ ...input, existingPlan })` → `writer.write()` 回原文件。

**删除逻辑：**

- `writeOrCandidate` 替换为内联幂等判断 + `writer.write()`。
- 删除 `outcomes.some(o => o === "candidate")` 的候选警告分支。

### 8. 引擎接口变更

三个引擎的输入类型增加可选的 `existing*` 字段：

**`TddEngine.generateDesign` — `DesignInput`：**

```typescript
interface DesignInput {
  spec: string;
  impact: string;
  codebaseSummary: string;
  packageStructure: string;
  architecture: string;
  existingDesign?: string; // 新增：已有设计文档内容
}
```

**`SpecEngine.generate` — `GenerateSpecInput`：**

```typescript
interface GenerateSpecInput {
  requirement: string;
  codebaseSummary: string;
  answers?: Record<string, string>;
  existingSpec?: {
    // 新增：已有规格制品
    spec: string;
    delta: string;
    model: SpecDocument;
  };
}
```

**`TddEngine.generatePlan` — `PlanningInput`：**

```typescript
interface PlanningInput {
  spec: string;
  design: string;
  impact: string;
  codebaseSummary: string;
  existingPlan?: {
    // 新增：已有计划制品
    tasksMarkdown: string;
    testPlan: string;
    context: string;
  };
}
```

当 `existing*` 存在时，引擎应在已有内容基础上编辑，尊重用户的手动修改，仅更新因输入变化而需要调整的部分。

### 9. 测试更新

| 测试文件                              | 改动                                                     |
| ------------------------------------- | -------------------------------------------------------- |
| `init-status.test.ts:709-783`         | 删除候选文件生成测试，改为验证直接覆盖                   |
| `init-agent-selection.test.ts:54-175` | 移除 `candidateFiles` 相关的断言                         |
| `loop.test.ts:30-61`                  | 删除 loop spec 候选保护测试，改为验证直接写入            |
| `design-plan.test.ts:218-305`         | 删除候选文件生成测试，改为验证就地合并                   |
| `new.test.ts:210-228,340-366`         | 删除候选文件生成测试，改为验证就地合并                   |
| `artifact-writer.test.ts:16-168`      | 删除 `writeOrCandidate` 和 `writeGroupOrCandidates` 测试 |

## 不变量

- 指令文件（CLAUDE.md / AGENTS.md）的行级去重追加策略不变。
- `ArtifactWriter.write()` 的写 `.meta.json` 行为不变。
- `inputHash` 幂等判断机制保留（仅从 `writeOrCandidate` 中提取到命令层）。
- `--force` 参数行为保持不变（直接覆盖，不走合并逻辑）。
