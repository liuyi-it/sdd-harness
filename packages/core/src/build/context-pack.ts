import { artifactInputHash } from "../artifacts/artifact-writer.js";
import type { ProjectRuleSnapshot } from "../project-conventions/rule-resolver.js";
import type { PolicyBundle, PolicyRef } from "@sdd-harness/agent-policies";
import { FIXED_SECURITY_RULES } from "../security/untrusted-content.js";

export interface ContextPackMetadata {
  schemaVersion: "2.0.0";
  codebaseIndexHash: string;
  sourceArtifactHash: string;
  projectRulesHash: string;
  projectConventionsHash: string;
  policyBundleHash?: string;
  contextPackDigest: string;
}

export interface ContextPackReferences {
  spec: string;
  design: string;
  plan: string;
  impact: string;
  codebase: string;
  domain?: string;
  adr?: string[];
  previousRun?: string;
}

export interface ContextPackTask {
  taskId: string;
  objective: string;
  userVisibleOutcome: string;
  requiredFiles: string[];
  allowedFiles: string[];
  forbiddenFiles: string[];
  verification: string[];
}

export function renderContextPack(input: {
  body: string;
  rules: ProjectRuleSnapshot;
  codebaseSummary: string;
  spec: string;
  design: string;
  impact: string;
  tasksMarkdown: string;
  tasksJson: string;
  projectConventionsHash: string;
  references: ContextPackReferences;
  task: ContextPackTask;
  policyBundle?: PolicyBundle;
}): string {
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
  const contextSection = renderContextSection(
    input.task,
    input.references,
    policyRefs,
  );
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
  const payload = truncateUtf8(
    `${securityRules}${rulesSection}${wrappedBody}`,
    30 * 1024,
  );
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

export function readContextPackMetadata(content: string): ContextPackMetadata {
  return {
    schemaVersion: requiredMatch(
      content,
      /^Schema Version: (2\.0\.0)$/m,
    ) as "2.0.0",
    codebaseIndexHash: requiredMatch(
      content,
      /^Codebase Index Hash: (sha256:[a-f0-9]{64})$/m,
    ),
    sourceArtifactHash: requiredMatch(
      content,
      /^Source Artifact Hash: (sha256:[a-f0-9]{64})$/m,
    ),
    projectRulesHash: requiredMatch(
      content,
      /^Project Rules Hash: (sha256:[a-f0-9]{64})$/m,
    ),
    projectConventionsHash: requiredMatch(
      content,
      /^Project Conventions Hash: (sha256:[a-f0-9]{64})$/m,
    ),
    ...optionalMatch(
      content,
      /^Policy Bundle Hash: (sha256:[a-f0-9]{64})$/m,
      "policyBundleHash",
    ),
    contextPackDigest: requiredMatch(
      content,
      /^Context Pack Digest: (sha256:[a-f0-9]{64})$/m,
    ),
  };
}

export function stripManagedSections(content: string): string {
  return content
    .replace(/^<!-- Context Pack Metadata[\s\S]*?-->\n*/u, "")
    .replace(
      /^<!-- Project Rules Start -->[\s\S]*?<!-- Project Rules End -->\n*/u,
      "",
    );
}

export function verifyContextPackDigest(content: string): boolean {
  try {
    const metadata = readContextPackMetadata(content);
    return (
      metadata.contextPackDigest === artifactInputHash(removeMetadata(content))
    );
  } catch {
    return false;
  }
}

function removeMetadata(content: string): string {
  return content.replace(/^<!-- Context Pack Metadata[\s\S]*?-->\n*/u, "");
}

function extractTaskBody(content: string): string {
  return (
    content.match(
      /<!-- Task Body Begin -->\n([\s\S]*?)\n<!-- Task Body End -->/u,
    )?.[1] ?? stripManagedSections(content)
  );
}

function requiredMatch(content: string, pattern: RegExp): string {
  const value = content.match(pattern)?.[1];
  if (value === undefined) throw new Error("Context Pack 元数据缺失");
  return value;
}

function optionalMatch<K extends string>(
  content: string,
  pattern: RegExp,
  key: K,
): Partial<Record<K, string>> {
  const value = content.match(pattern)?.[1];
  return value === undefined ? {} : ({ [key]: value } as Record<K, string>);
}

function validateReferences(references: ContextPackReferences): void {
  const paths = [
    references.spec,
    references.design,
    references.plan,
    references.impact,
    references.codebase,
    references.domain,
    references.previousRun,
    ...(references.adr ?? []),
  ].filter((value): value is string => value !== undefined);
  for (const path of paths) {
    if (
      path.length === 0 ||
      path.startsWith("/") ||
      path.split(/[\\/]/u).includes("..")
    ) {
      throw new Error(`Context Pack 引用必须是仓库内相对路径：${path}`);
    }
  }
}

function renderContextSection(
  task: ContextPackTask,
  references: ContextPackReferences,
  policyRefs: PolicyRef[],
): string {
  const referenceEntries = Object.entries(references).flatMap(([key, value]) =>
    value === undefined
      ? []
      : Array.isArray(value)
        ? value.map((path) => `- ${key}: ${path}`)
        : [`- ${key}: ${value}`],
  );
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
    ...policyRefs.map(
      (policy) => `- ${policy.id}@${policy.version} (${policy.digest})`,
    ),
  ].join("\n");
}

function truncateUtf8(value: string, maxBytes: number): string {
  if (Buffer.byteLength(value) <= maxBytes) return value;
  let end = value.length;
  while (end > 0 && Buffer.byteLength(value.slice(0, end)) > maxBytes - 32) {
    end -= 1;
  }
  return `${value.slice(0, end)}\n\n[Context truncated]\n`;
}
