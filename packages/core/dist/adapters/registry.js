import { compileBaseSkill, compileCommandTemplate, compileInstruction, } from "@sdd-harness/agent-policies";
const ADAPTER_PACKAGES = [
    "@sdd-harness/claude-code-adapter",
    "@sdd-harness/codex-adapter",
    "@sdd-harness/opencode-adapter",
];
const BUILTIN_DESCRIPTORS = {
    "@sdd-harness/claude-code-adapter": descriptor("claude", "CLAUDE.md", ".claude"),
    "@sdd-harness/codex-adapter": descriptor("codex", "AGENTS.md", ".codex"),
    "@sdd-harness/opencode-adapter": descriptor("opencode", "AGENTS.md", ".opencode"),
};
function descriptor(agent, instructionFile, root) {
    return {
        agent,
        instructionFile,
        commandsDir: `${root}/commands`,
        skillsDir: `${root}/skills/sdd-harness`,
        capabilities: {
            supportsSkills: true,
            supportsModelInvocation: true,
            supportsUserCommands: true,
            supportsReferences: true,
        },
    };
}
export async function getAvailableAdapters() {
    const manifests = await Promise.all(ADAPTER_PACKAGES.map(async (packageName) => {
        const loaded = await loadDescriptor(packageName);
        return compileManifest(loaded.descriptor, loaded.degradationReason);
    }));
    return manifests;
}
async function loadDescriptor(packageName) {
    try {
        const module = await import(`${packageName}/manifest.json`, {
            with: { type: "json" },
        });
        if (isValidDescriptor(module.default))
            return { descriptor: module.default };
    }
    catch {
        // 安装包不可见时使用相同的内置宿主描述，并显式记录降级。
    }
    return {
        descriptor: BUILTIN_DESCRIPTORS[packageName],
        degradationReason: `无法加载 ${packageName}/manifest.json，已使用内置宿主描述`,
    };
}
function compileManifest(descriptor, degradationReason) {
    const warnings = [];
    if (!descriptor.capabilities.supportsSkills) {
        warnings.push(`${descriptor.agent} 不支持 Skill，阶段 Policy 将通过 actionRequired 与 Context Pack 注入`);
    }
    if (degradationReason !== undefined)
        warnings.push(degradationReason);
    const agent = `${descriptor.agent}Adapter`;
    return {
        ...descriptor,
        commandsDir: descriptor.commandsDir ?? `.${descriptor.agent}/commands`,
        instructionContent: compileInstruction(agent),
        commandTemplate: compileCommandTemplate(agent),
        ...(descriptor.capabilities.supportsSkills &&
            descriptor.skillsDir !== undefined
            ? { skillContent: compileBaseSkill() }
            : {}),
        warnings,
        ...(degradationReason === undefined ? {} : { degradationReason }),
    };
}
function isValidDescriptor(value) {
    if (value === null || typeof value !== "object")
        return false;
    const descriptor = value;
    const capabilities = descriptor.capabilities;
    return (typeof descriptor.agent === "string" &&
        descriptor.agent.length > 0 &&
        typeof descriptor.instructionFile === "string" &&
        capabilities !== null &&
        typeof capabilities === "object" &&
        [
            "supportsSkills",
            "supportsModelInvocation",
            "supportsUserCommands",
            "supportsReferences",
        ].every((key) => typeof capabilities[key] === "boolean"));
}
//# sourceMappingURL=registry.js.map