# 第三方声明

本项目会与下列固定版本的上游项目交互，或复用其工作流思想。

| 项目                | 版本   | Commit                                     | 许可证 | 使用方式                                           |
| ------------------- | ------ | ------------------------------------------ | ------ | -------------------------------------------------- |
| codebase-memory-mcp | v0.8.1 | `f0c9be19c5d74b84f418d807bfdce7b5d6a261ff` | MIT    | 作为外部 MCP 运行时接入，不 vendor 上游源码        |
| OpenSpec            | v1.4.1 | `1b06fddd59d8e592d5b5794a1970b22867e85b1f` | MIT    | 在 `vendor/openspec/upstream/` 保存完整源码快照    |
| Superpowers         | v6.1.1 | `d884ae04edebef577e82ff7c4e143debd0bbec99` | MIT    | 在 `vendor/superpowers/upstream/` 保存完整源码快照 |

说明：

- `vendor/openspec/upstream/LICENSE` 保留 OpenSpec 许可证文本
- `vendor/superpowers/upstream/LICENSE` 保留 Superpowers 许可证文本
- 两个快照分别通过同目录的 `VERSION.json` 固定来源，通过 `MANIFEST.sha256` 校验完整性
- 运行时不会直接调用 OpenSpec 或 Superpowers 的上游可执行代码
