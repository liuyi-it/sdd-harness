# Architecture

```text
Claude Code command ─┐
                     ├─ HostAdapter ─ Core ─ State / Artifacts / Git / MCP
Codex skill ─────────┘                 ├─ SpecEngine
                                      ├─ TddEngine
                                      └─ Quality Gates
```

`@sdd-harness/core` is the only component allowed to change workflow state. Platform adapters parse host syntax and return the Core `CommandResult` unchanged. The host supplies `McpTransport` and `TaskExecutor`; this keeps external tool execution explicit and testable.

Core writes all workflow facts under `.sdd/`. Markdown artifacts have adjacent metadata containing input and artifact SHA-256 values. State updates use temporary-file rename plus backup recovery, and every write command takes `.sdd/lock`.
