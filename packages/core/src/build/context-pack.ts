import { artifactInputHash } from "../artifacts/artifact-writer.js";
import type { ProjectRuleSnapshot } from "../project-conventions/rule-resolver.js";
import {
  FIXED_SECURITY_RULES,
  wrapUntrustedMcpOutput,
  wrapUntrustedRepositoryContent,
} from "../security/untrusted-content.js";

export interface ContextPackMetadata {
  codebaseIndexHash: string;
  sourceArtifactHash: string;
  projectRulesHash: string;
  projectConventionsHash: string;
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
}): string {
  const metadata = [
    "<!-- Context Pack Metadata",
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
    `Generated At: ${new Date().toISOString()}`,
    "-->",
    "",
  ].join("\n");
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
  const wrappedBody = [
    wrapUntrustedRepositoryContent(input.spec, "spec.md"),
    wrapUntrustedRepositoryContent(input.design, "design.md"),
    wrapUntrustedRepositoryContent(input.impact, "impact.md"),
    wrapUntrustedMcpOutput(input.codebaseSummary, "init/architecture"),
    "",
    "<!-- Task Body Begin -->",
    stripManagedSections(input.body),
    "<!-- Task Body End -->",
  ].join("\n\n");
  return truncateUtf8(
    `${metadata}${securityRules}${rulesSection}${wrappedBody}`,
    30 * 1024,
  );
}

export function readContextPackMetadata(content: string): ContextPackMetadata {
  return {
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

function requiredMatch(content: string, pattern: RegExp): string {
  const value = content.match(pattern)?.[1];
  if (value === undefined) throw new Error("Context Pack 元数据缺失");
  return value;
}

function truncateUtf8(value: string, maxBytes: number): string {
  if (Buffer.byteLength(value) <= maxBytes) return value;
  let end = value.length;
  while (end > 0 && Buffer.byteLength(value.slice(0, end)) > maxBytes - 32) {
    end -= 1;
  }
  return `${value.slice(0, end)}\n\n[Context truncated]\n`;
}
