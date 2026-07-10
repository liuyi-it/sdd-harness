/**
 * 适配器安装清单，由各 adapter 包的 manifest.json 提供。
 * 描述该 agent 需要的集成文件及内容。
 */
export interface AdapterManifest {
    /** 适配器标识，如 "claude"、"codex"、"opencode" */
    agent: string;
    /** 指令文件名，如 "CLAUDE.md"、"AGENTS.md" */
    instructionFile: string;
    /** 追加到指令文件的内容（含 <!-- sdd-harness:managed --> 标记） */
    instructionContent: string;
    /** commands 目录路径，如 ".claude/commands" */
    commandsDir: string;
    /** command 文件模板，{command} 会被替换为具体命令名 */
    commandTemplate: string;
    /** skills 目录路径（可选），如 ".claude/skills/sdd-harness" */
    skillsDir?: string;
    /** SKILL.md 内容（可选） */
    skillContent?: string;
    /** rules 文件列表（可选） */
    rules?: Array<{
        file: string;
        content: string;
    }>;
}
//# sourceMappingURL=types.d.ts.map