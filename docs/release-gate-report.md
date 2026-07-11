# Release Gate Report

## Result

PASS

## 基线与范围

- 验收基线：`2077668088ee1d883319d94d0bef3392275c2c38`
- 最终提交：本报告随本次修复提交
- 覆盖范围：Agent handoff、状态机与 1.4 迁移、MCP 生命周期/分页/解码/降级身份、CLI 与三平台 CI、发布 tarball。

## 关键修复

- `build next/complete` 持久化 Git baseline，以真实 delta 取代 Agent 自报文件；越权、隐瞒、新增和删除均被拦截。
- WorkflowState 升级至 1.4.0，旧 waiting handoff 无可信基线时结构化失败；可恢复 handoff 记录用于历史 run 恢复。
- MCP 支持 tools/list 分页、Content-Length 与 JSONL 双分帧、真实 capability、typed decoder、空精确结果及 fallback 条目透传。
- stop/restart 使旧 handoff 失效；auto 增加重试与无进展保护；CI 覆盖 Linux/macOS/Windows Node 22 全部门禁。

## AC-001 至 AC-030

| 项目 | 状态 | 自动化或运行证据 |
| --- | --- | --- |
| AC-001–004 | PASS | `auto.test.ts`、`build.test.ts` |
| AC-005–010 | PASS | Git delta、handoff、restart/stop/resume 回归测试 |
| AC-011–014 | PASS | FileLock、状态恢复与 1.0/1.2/1.3 → 1.4 测试 |
| AC-015–020 | PASS | `lifecycle.test.ts`、真实 MCP index/query/60 秒 smoke |
| AC-021–024 | PASS | task/result schema、TDD、V2 round-trip 测试 |
| AC-025–029 | PASS | archive、status/doctor、错误码、路径和命令安全测试 |
| AC-030 | PASS | 全量门禁、workspace tarball 本地安装 smoke |

## 实际执行命令与退出码

| 命令 | 退出码 |
| --- | --- |
| `npm ci` | 0 |
| `npm run format:check` | 0 |
| `npm run lint` | 0 |
| `npm run typecheck` | 0 |
| `npm run validate:schemas` | 0 |
| `npm test` | 0（35 files / 360 tests） |
| `npm run validate:release` | 0 |
| `npm run build` | 0 |
| `git diff --check` | 0 |
| `npm pack --workspace ...` | 0 |
| 临时安装 tarball；`sdd --version`、`sdd --help` | 0 |
| 真实 MCP `initialize → tools/list → index → query` | 0 |
| 真实 MCP 保持 60 秒后 query | 0 |

## CI

CI 配置为 `ubuntu-latest`、`macos-latest`、`windows-latest` × Node 22，并运行安装、格式、lint、类型、Schema、测试、发布校验、构建和 dist 一致性检查。Windows 同时覆盖 CLI help、init、build handoff 与 MCP Windows 启动参数单元测试。

## 剩余风险

无已知 P0/P1。MCP 首次运行可能下载其独立二进制；受限网络会进入身份明确的 fallback-file-scan，而 `requireAvailable=true` 会返回 `E_COMPONENT_UNAVAILABLE`。
