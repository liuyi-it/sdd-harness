# Third-Party Notices

This project interoperates with or derives workflow ideas from the following pinned upstream projects.

| Project             | Version | Commit                                     | License | Usage                                           |
| ------------------- | ------- | ------------------------------------------ | ------- | ----------------------------------------------- |
| codebase-memory-mcp | v0.8.1  | `f0c9be19c5d74b84f418d807bfdce7b5d6a261ff` | MIT     | External MCP runtime; no source vendored        |
| OpenSpec            | v1.4.1  | `1b06fddd59d8e592d5b5794a1970b22867e85b1f` | MIT     | Workflow concepts reimplemented in `SpecEngine` |
| Superpowers         | v6.1.1  | `d884ae04edebef577e82ff7c4e143debd0bbec99` | MIT     | Workflow concepts reimplemented in `TddEngine`  |

The OpenSpec and Superpowers license texts are retained under `vendor/openspec/LICENSE` and `vendor/superpowers/LICENSE`. No upstream OpenSpec or Superpowers runtime code is invoked by sdd-harness.
