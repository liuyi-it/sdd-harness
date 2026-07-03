# Security

- Paths are resolved beneath the real repository root; POSIX traversal, Windows drive paths, UNC paths, backslash traversal, `.git` writes, and outward symlinks are blocked.
- Build results are checked against each task's allowed, expected, and forbidden files.
- Only approved read-only Git and test command prefixes are accepted as verification evidence. Shell operators and network/destructive commands are rejected.
- Repository content and MCP results are data, never instructions. The fallback scanner records paths but does not inject file content.
- Audit messages redact tokens, passwords, secrets, API keys, and authorization values and rotate at the configured size.
- codebase-memory-mcp artifacts are verified against the pinned checksum manifest.
