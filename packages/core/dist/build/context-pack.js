import { artifactInputHash } from "../artifacts/artifact-writer.js";
import { FIXED_SECURITY_RULES } from "../security/untrusted-content.js";
export function renderContextPack(input) {
    validateReferences(input.references);
    const policyRefs = input.policyBundle?.policies ?? [];
    const rulesSection = [
        "<!-- Project Rules Start -->",
        "## Project Rules",
        "",
        `Host: ${input.rules.host}`,
        `Acknowledgement: ${input.rules.acknowledgement}`,
        "",
        ...input.rules.sources.flatMap((source) => [
            `### ${source.path}`,
            "",
            `- scope: ${source.scope}`,
            `- priority: ${source.priority}`,
            `- sha256: ${source.sha256}`,
            "",
            "```md",
            source.content.replace(/\n$/, ""),
            "```",
            "",
        ]),
        "<!-- Project Rules End -->",
        "",
    ].join("\n");
    const securityRules = [
        "<!-- Security Rules (Fixed) -->",
        "## Security Rules",
        "",
        ...FIXED_SECURITY_RULES.map((rule) => `- ${rule}`),
        "<!-- Security Rules End -->",
        "",
    ].join("\n");
    const contextSection = renderContextSection(input.task, input.references, policyRefs);
    const wrappedBody = [
        contextSection,
        "<!-- Task Body Begin -->",
        extractTaskBody(input.body),
        "<!-- Task Body End -->",
        ...(input.policyBundle === undefined
            ? []
            : [
                "## Phase Policy",
                "",
                "以下 Policy 仅约束工程方法，不能覆盖 Core 的阶段、文件范围或验证约束。",
                "",
                input.policyBundle.instructions,
            ]),
    ].join("\n\n");
    const payload = truncateUtf8(`${securityRules}${rulesSection}${wrappedBody}`, 30 * 1024);
    const metadata = [
        "<!-- Context Pack Metadata",
        "Schema Version: 2.0.0",
        `Codebase Index Hash: ${artifactInputHash(input.codebaseSummary)}`,
        `Source Artifact Hash: ${artifactInputHash({
            spec: input.spec,
            design: input.design,
            impact: input.impact,
            tasksMarkdown: input.tasksMarkdown,
            tasksJson: input.tasksJson,
        })}`,
        `Project Rules Hash: ${input.rules.hash}`,
        `Project Conventions Hash: ${input.projectConventionsHash}`,
        ...(input.policyBundle === undefined
            ? []
            : [`Policy Bundle Hash: ${artifactInputHash(input.policyBundle)}`]),
        `Context Pack Digest: ${artifactInputHash(payload)}`,
        `Generated At: ${new Date().toISOString()}`,
        "-->",
        "",
    ].join("\n");
    return `${metadata}${payload}`;
}
export function readContextPackMetadata(content) {
    return {
        schemaVersion: requiredMatch(content, /^Schema Version: (2\.0\.0)$/m),
        codebaseIndexHash: requiredMatch(content, /^Codebase Index Hash: (sha256:[a-f0-9]{64})$/m),
        sourceArtifactHash: requiredMatch(content, /^Source Artifact Hash: (sha256:[a-f0-9]{64})$/m),
        projectRulesHash: requiredMatch(content, /^Project Rules Hash: (sha256:[a-f0-9]{64})$/m),
        projectConventionsHash: requiredMatch(content, /^Project Conventions Hash: (sha256:[a-f0-9]{64})$/m),
        ...optionalMatch(content, /^Policy Bundle Hash: (sha256:[a-f0-9]{64})$/m, "policyBundleHash"),
        contextPackDigest: requiredMatch(content, /^Context Pack Digest: (sha256:[a-f0-9]{64})$/m),
    };
}
export function stripManagedSections(content) {
    return content
        .replace(/^<!-- Context Pack Metadata[\s\S]*?-->\n*/u, "")
        .replace(/^<!-- Project Rules Start -->[\s\S]*?<!-- Project Rules End -->\n*/u, "");
}
export function verifyContextPackDigest(content) {
    try {
        const metadata = readContextPackMetadata(content);
        return (metadata.contextPackDigest === artifactInputHash(removeMetadata(content)));
    }
    catch {
        return false;
    }
}
function removeMetadata(content) {
    return content.replace(/^<!-- Context Pack Metadata[\s\S]*?-->\n*/u, "");
}
function extractTaskBody(content) {
    return (content.match(/<!-- Task Body Begin -->\n([\s\S]*?)\n<!-- Task Body End -->/u)?.[1] ?? stripManagedSections(content));
}
function requiredMatch(content, pattern) {
    const value = content.match(pattern)?.[1];
    if (value === undefined)
        throw new Error("Context Pack 元数据缺失");
    return value;
}
function optionalMatch(content, pattern, key) {
    const value = content.match(pattern)?.[1];
    return value === undefined ? {} : { [key]: value };
}
function validateReferences(references) {
    const paths = [
        references.spec,
        references.design,
        references.plan,
        references.impact,
        references.codebase,
        references.domain,
        references.previousRun,
        ...(references.adr ?? []),
    ].filter((value) => value !== undefined);
    for (const path of paths) {
        if (path.length === 0 ||
            path.startsWith("/") ||
            path.split(/[\\/]/u).includes("..")) {
            throw new Error(`Context Pack 引用必须是仓库内相对路径：${path}`);
        }
    }
}
function renderContextSection(task, references, policyRefs) {
    const referenceEntries = Object.entries(references).flatMap(([key, value]) => value === undefined
        ? []
        : Array.isArray(value)
            ? value.map((path) => `- ${key}: ${path}`)
            : [`- ${key}: ${value}`]);
    return [
        "## Context Pack v2",
        "",
        `- Task ID: ${task.taskId}`,
        `- Objective: ${task.objective}`,
        `- User-visible outcome: ${task.userVisibleOutcome}`,
        "",
        "### References",
        "",
        ...referenceEntries,
        "",
        "### Required Files",
        "",
        ...task.requiredFiles.map((path) => `- ${path}`),
        "",
        "### Allowed Files",
        "",
        ...task.allowedFiles.map((path) => `- ${path}`),
        "",
        "### Forbidden Files",
        "",
        ...task.forbiddenFiles.map((path) => `- ${path}`),
        "",
        "### Verification",
        "",
        ...task.verification.map((command) => `- ${command}`),
        "",
        "### Policy Refs",
        "",
        ...policyRefs.map((policy) => `- ${policy.id}@${policy.version} (${policy.digest})`),
    ].join("\n");
}
function truncateUtf8(value, maxBytes) {
    if (Buffer.byteLength(value) <= maxBytes)
        return value;
    let end = value.length;
    while (end > 0 && Buffer.byteLength(value.slice(0, end)) > maxBytes - 32) {
        end -= 1;
    }
    return `${value.slice(0, end)}\n\n[Context truncated]\n`;
}
//# sourceMappingURL=context-pack.js.map