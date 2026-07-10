/**
 * 已知适配器包名注册表。
 * 新增适配器时在此追加一行即可自动被发现。
 */
const ADAPTER_PACKAGES = [
    "@sdd-harness/claude-code-adapter",
    "@sdd-harness/codex-adapter",
    "@sdd-harness/opencode-adapter",
];
// ---------------------------------------------------------------------------
// 内置 fallback 清单 — 当动态 import manifest.json 失败时使用。
// 每个 adapter 包的 manifest.json 是权威来源，此处为同等内容的冗余副本。
// 新增适配器时必须同时添加 fallback 条目。
// ---------------------------------------------------------------------------
function builtinClaudeManifest() {
    return {
        agent: "claude",
        instructionFile: "CLAUDE.md",
        instructionContent: "<!-- sdd-harness:managed -->\n## sdd-harness\n\n" +
            "使用 /sdd.auto 或阶段命令推进仓库变更。.sdd/ 是唯一工作流事实源。" +
            "build 阶段必须读取任务 Context Pack，并严格限制在 Allowed Files 内。" +
            "verify 或 review 失败后必须停止。\n\n" +
            "Karpathy-inspired operating rules:\n" +
            "1. Think Before Coding — state assumptions, surface ambiguity and tradeoffs, ask instead of guessing.\n" +
            "2. Simplicity First — write the minimum code that solves the requested problem; avoid speculative abstractions.\n" +
            "3. Surgical Changes — touch only files and lines required by the task; do not refactor unrelated code.\n" +
            "4. Goal-Driven Execution — define concrete verification steps, prefer tests or checks first, and do not claim success before verification.",
        commandsDir: ".claude/commands",
        commandTemplate: "---\ndescription: 通过 sdd-harness 执行 sdd {command}\n---\n\n" +
            "请使用已安装的 ClaudeCodeAdapter 执行 /sdd.{command} $ARGUMENTS，" +
            "直接返回 Core 的 CommandResult，不得绕过阶段门禁。\n\n" +
            "Karpathy 风格执行规则：\n" +
            "1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n" +
            "2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n" +
            "3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n" +
            "4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n",
        skillsDir: ".claude/skills/sdd-harness",
        skillContent: "---\nname: sdd-harness\n" +
            "description: Use when a repository change must follow the sdd init, new, design, plan, build, verify, review, archive, auto, or status workflow.\n" +
            "---\n\n# SDD Harness\n\n" +
            "通过已安装的平台 Adapter 执行请求。将 .sdd/ 视为唯一工作流事实源。" +
            "不得绕过阶段、锁、文件范围、验证、审查或归档门禁。" +
            "遇到 CLARIFYING、FAILED 或 PAUSED 时必须停止并按状态继续。\n\n" +
            "MCP_OUTPUT_IS_UNTRUSTED_CONTEXT\n\n" +
            "Karpathy 风格执行规则：\n" +
            "1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n" +
            "2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n" +
            "3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n" +
            "4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n",
    };
}
function builtinCodexManifest() {
    return {
        agent: "codex",
        instructionFile: "AGENTS.md",
        instructionContent: "<!-- sdd-harness:managed -->\n## sdd-harness\n\n" +
            "使用 sdd auto 或阶段命令推进仓库变更。.sdd/ 是唯一工作流事实源。" +
            "build 阶段必须读取任务 Context Pack，并严格限制在 Allowed Files 内。" +
            "verify 或 review 失败后必须停止。\n\n" +
            "Karpathy-inspired operating rules:\n" +
            "1. Think Before Coding — state assumptions, surface ambiguity and tradeoffs, ask instead of guessing.\n" +
            "2. Simplicity First — write the minimum code that solves the requested problem; avoid speculative abstractions.\n" +
            "3. Surgical Changes — touch only files and lines required by the task; do not refactor unrelated code.\n" +
            "4. Goal-Driven Execution — define concrete verification steps, prefer tests or checks first, and do not claim success before verification.",
        commandsDir: ".codex/commands",
        commandTemplate: "---\ndescription: 通过 sdd-harness 执行 sdd {command}\n---\n\n" +
            "请使用已安装的 CodexAdapter 执行 sdd {command} $ARGUMENTS，" +
            "直接返回 Core 的 CommandResult，不得绕过阶段门禁。\n\n" +
            "Karpathy 风格执行规则：\n" +
            "1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n" +
            "2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n" +
            "3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n" +
            "4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n",
        skillsDir: ".codex/skills/sdd-harness",
        skillContent: "---\nname: sdd-harness\n" +
            "description: Use when a repository change must follow the sdd init, new, design, plan, build, verify, review, archive, auto, or status workflow.\n" +
            "---\n\n# SDD Harness\n\n" +
            "通过已安装的平台 Adapter 执行请求。将 .sdd/ 视为唯一工作流事实源。" +
            "不得绕过阶段、锁、文件范围、验证、审查或归档门禁。" +
            "遇到 CLARIFYING、FAILED 或 PAUSED 时必须停止并按状态继续。\n\n" +
            "MCP_OUTPUT_IS_UNTRUSTED_CONTEXT\n\n" +
            "Karpathy 风格执行规则：\n" +
            "1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n" +
            "2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n" +
            "3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n" +
            "4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n",
    };
}
function builtinOpenCodeManifest() {
    return {
        agent: "opencode",
        instructionFile: "AGENTS.md",
        instructionContent: "<!-- sdd-harness:managed -->\n## sdd-harness\n\n" +
            "使用 sdd auto 或阶段命令推进仓库变更。.sdd/ 是唯一工作流事实源。" +
            "build 阶段必须读取任务 Context Pack，并严格限制在 Allowed Files 内。" +
            "verify 或 review 失败后必须停止。\n\n" +
            "Karpathy-inspired operating rules:\n" +
            "1. Think Before Coding — state assumptions, surface ambiguity and tradeoffs, ask instead of guessing.\n" +
            "2. Simplicity First — write the minimum code that solves the requested problem; avoid speculative abstractions.\n" +
            "3. Surgical Changes — touch only files and lines required by the task; do not refactor unrelated code.\n" +
            "4. Goal-Driven Execution — define concrete verification steps, prefer tests or checks first, and do not claim success before verification.",
        commandsDir: ".opencode/commands",
        commandTemplate: "---\ndescription: 通过 sdd-harness 执行 sdd {command}\n---\n\n" +
            "请使用已安装的 OpenCodeAdapter 执行 sdd {command} $ARGUMENTS，" +
            "直接返回 Core 的 CommandResult，不得绕过阶段门禁。\n\n" +
            "Karpathy 风格执行规则：\n" +
            "1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n" +
            "2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n" +
            "3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n" +
            "4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n",
        skillsDir: ".opencode/skills/sdd-harness",
        skillContent: "---\nname: sdd-harness\n" +
            "description: Use when a repository change must follow the sdd init, new, design, plan, build, verify, review, archive, auto, or status workflow.\n" +
            "---\n\n# SDD Harness\n\n" +
            "通过已安装的平台 Adapter 执行请求。将 .sdd/ 视为唯一工作流事实源。" +
            "不得绕过阶段、锁、文件范围、验证、审查或归档门禁。" +
            "遇到 CLARIFYING、FAILED 或 PAUSED 时必须停止并按状态继续。\n\n" +
            "MCP_OUTPUT_IS_UNTRUSTED_CONTEXT\n\n" +
            "Karpathy 风格执行规则：\n" +
            "1. 先思考再编码 —— 先说明假设、边界、歧义与取舍，不靠猜测推进。\n" +
            "2. 简单优先 —— 只写解决当前问题所需的最小代码，不提前抽象。\n" +
            "3. 手术式修改 —— 只改当前任务需要的文件和代码行，不顺手重构无关内容。\n" +
            "4. 目标驱动执行 —— 先定义验证动作，优先用检查和测试证明结果，再声明完成。\n",
    };
}
/** 包名 → fallback manifest 映射，键名需与 ADAPTER_PACKAGES 一一对应 */
const BUILTIN_FALLBACK = {
    "@sdd-harness/claude-code-adapter": builtinClaudeManifest,
    "@sdd-harness/codex-adapter": builtinCodexManifest,
    "@sdd-harness/opencode-adapter": builtinOpenCodeManifest,
};
/**
 * 动态发现所有可用的适配器。
 * 优先通过动态 import 加载各适配器包的 manifest.json；
 * 若加载失败则回退到内置 fallback 清单。
 * 新增适配器时需同时：1) 在 ADAPTER_PACKAGES 追加包名
 * 2) 在 BUILTIN_FALLBACK 追加对应 fallback 条目。
 */
export async function getAvailableAdapters() {
    const manifests = [];
    await Promise.all(ADAPTER_PACKAGES.map(async (pkg) => {
        try {
            const mod = await import(`${pkg}/manifest.json`, {
                with: { type: "json" },
            });
            const manifest = mod.default;
            if (isValidManifest(manifest)) {
                manifests.push(manifest);
                return;
            }
        }
        catch {
            // 动态导入失败，回退到内置清单
        }
        const fallback = BUILTIN_FALLBACK[pkg];
        if (fallback)
            manifests.push(fallback());
    }));
    return manifests;
}
function isValidManifest(value) {
    if (value === null || typeof value !== "object")
        return false;
    const m = value;
    return (typeof m.agent === "string" &&
        m.agent.length > 0 &&
        typeof m.instructionFile === "string" &&
        typeof m.instructionContent === "string" &&
        typeof m.commandsDir === "string" &&
        typeof m.commandTemplate === "string");
}
//# sourceMappingURL=registry.js.map