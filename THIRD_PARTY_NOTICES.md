# 第三方声明

本项目保留下列上游快照或调用外部工具：

| 项目 | 固定来源 | 使用方式 |
| --- | --- | --- |
| OpenSpec | `vendor/openspec/VERSION.json` | 保存审计快照，运行时不执行上游代码 |
| Superpowers | `vendor/superpowers/VERSION.json` | 保存审计快照，运行时不执行上游代码 |
| mattpocock/skills | `vendor/mattpocock-skills/` | 保留来源与许可证，工程方法改写为受控 Policy |
| Ponytail | `docs/upstream/ponytail.md` | 仅改写最小正确实现思想，不安装插件、Hook 或运行时 |
| GitNexus | 用户 PATH 中的外部 CLI | 可选工具，不随本项目分发；npm 包当前要求 Node.js 22+，采用 PolyForm Noncommercial 许可证，商业使用需单独评估 |
| CodeGraph | 用户 PATH 中的外部 CLI | 可选工具，不随本项目分发；独立安装包无需 Node.js，当前 npm 包采用 MIT 许可证 |

各 vendor 目录保留对应许可证、版本和完整性清单。Cargo 依赖的具体版本记录在 `Cargo.lock`，许可证以各 crate 随附声明为准。
