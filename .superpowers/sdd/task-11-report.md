## Task 11 实现报告

### 变更范围

- 文件: `packages/core/test/init-agent-selection.test.ts`
- 文件: `packages/core/test/init-status.test.ts`
- 文件: `packages/core/test/design-plan.test.ts`
- 文件: `packages/core/test/new.test.ts`

### 实现内容

1. **`init-agent-selection.test.ts`（4 处）**
   - `installProjectIntegration()` 返回类型从 `ProjectIntegrationResult` 变为 `void`
   - 移除 4 处 `expect(result.candidateFiles).toEqual([])` 断言
   - 将 `const result = await` 改为直接 `await`

2. **`init-status.test.ts`（3 处）**
   - "is idempotent, preserves user config..."：移除 `custom: keep` 保留断言（config 现在被直接覆盖）
   - "writes candidate files..."：重命名为 "writes integration files with line-dedup merge for manually edited files"，移除候选文件警告断言；命令文件被直接覆盖（受管文件），CLAUDE.md 仍执行行级去重合并
   - "fails init when config.yml misses required fields"：重命名为 "recovers from invalid config.yml by overwriting with default config"，改为期望成功（config 在验证前被直接覆盖）

3. **`design-plan.test.ts`（2 处）**
   - "returns already ready... candidate when input changed"：重写为 "regenerates with existing design merge"，移除候选警告和保留原稿断言；验证 design 使用已有内容合并重新生成
   - "protects planned artifacts from changed inputs"：重写为 "regenerates plan artifacts with merge when input changes"，移除候选警告；计划制品直接合并覆盖

4. **`new.test.ts`（2 处）**
   - "protects manually edited structured artifacts and honors force"：移除 `warnings` 候选断言和手动内容保留断言；验证无候选文件生成
   - "repeats structured artifact generation idempotently"：将 `.toBe(before)`（严格字符串比较含时间戳）改为 `.toMatchObject`（仅比较 key 字段）

### 验证结果

- `npm run typecheck` — 通过
- `npm run build` — 通过
- `npm test` — **341/341 全部通过，0 失败**
- `npm run lint` — 待确认

### 受影响的测试文件汇总

| 文件                         | 修改前失败数 | 修改后   |
| ---------------------------- | ------------ | -------- |
| init-agent-selection.test.ts | 4            | 全部通过 |
| init-status.test.ts          | 3            | 全部通过 |
| design-plan.test.ts          | 2            | 全部通过 |
| new.test.ts                  | 2            | 全部通过 |
