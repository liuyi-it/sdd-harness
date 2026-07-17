export * from "./types.js";
export { CodebaseMemoryManager } from "./manager.js";
export type { InitResult } from "./manager.js";
export {
  startManagedMcp,
  stopManagedMcp,
  resolveInstalledMcp,
  type InstalledMcp,
  type McpSpawnSpec,
  type McpProgressReporter,
  type ResolveInstalledMcpOptions,
  type StartManagedMcpOptions,
} from "./lifecycle.js";
export {
  writeDiagnostics,
  createDiagnostics,
  createDiagError,
} from "./diagnostics.js";
export { fallbackQuery } from "./fallback-bridge.js";
export {
  degradedWarning,
  startFailedWarning,
  WARNING_CODES,
} from "./warnings.js";
export { CodebaseMemoryTransport } from "./transport.js";
