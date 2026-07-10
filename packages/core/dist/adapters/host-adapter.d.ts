import { type CommandName, type CommandResult, type SddCore } from "../contracts.js";
/**
 * HostAdapter 把 Claude Code / Codex 的命令字符串解析成统一的 Core 请求。
 * 适配器层不保存工作流状态，也不改写 Core 的语义结果。
 */
export type HostStyle = "claude-code" | "codex";
export interface PluginVersion {
    name: "sdd-harness";
    version: string;
    delivery: "plugin";
    supportedTargets: ["claude-code", "codex"];
}
export declare class HostAdapter {
    private readonly core;
    private readonly style;
    constructor(core: SddCore, style: HostStyle);
    execute(input: string, cwd: string): Promise<CommandResult>;
    version(): PluginVersion;
}
interface ParsedCommand {
    command: CommandName;
    args: Record<string, unknown>;
    help: boolean;
    version: boolean;
}
export declare function parseHostCommand(input: string, cwd: string, style: HostStyle): ParsedCommand;
export {};
//# sourceMappingURL=host-adapter.d.ts.map