import { readFile } from "node:fs/promises";
import { join } from "node:path";

import type { SpecDocument } from "../engines/openspec/model.js";
import type { TaskDefinition } from "../engines/superpowers/protocol.js";
import type { PlannedDependency } from "../engines/superpowers/protocol.js";
import { SddError } from "../errors.js";

export interface CompactSpec {
  schemaVersion: "2.0.0";
  status: "CLARIFYING" | "READY";
  requirement: string;
  proposal: string;
  impact: string;
  questions: string;
  answers?: string;
  assumptions?: string;
  delta?: string;
  model?: SpecDocument;
}

export interface CompactPlan {
  schemaVersion: "2.0.0";
  tasks: TaskDefinition[];
  tasksMarkdown: string;
  testPlan: string;
  context: string;
  dependencies?: PlannedDependency[];
}

export async function readCompactSpec(change: string): Promise<CompactSpec> {
  const value = await readJson(join(change, "spec.json"), "spec.json");
  if (
    typeof value !== "object" ||
    value === null ||
    (value as Partial<CompactSpec>).schemaVersion !== "2.0.0" ||
    !["CLARIFYING", "READY"].includes(
      String((value as Partial<CompactSpec>).status),
    ) ||
    typeof (value as Partial<CompactSpec>).requirement !== "string" ||
    typeof (value as Partial<CompactSpec>).proposal !== "string" ||
    typeof (value as Partial<CompactSpec>).impact !== "string" ||
    typeof (value as Partial<CompactSpec>).questions !== "string"
  )
    throw corrupted("spec.json 结构无效");
  return value as CompactSpec;
}

export async function readCompactPlan(change: string): Promise<CompactPlan> {
  const value = await readJson(join(change, "plan.json"), "plan.json");
  if (
    typeof value !== "object" ||
    value === null ||
    (value as Partial<CompactPlan>).schemaVersion !== "2.0.0" ||
    !Array.isArray((value as Partial<CompactPlan>).tasks) ||
    typeof (value as Partial<CompactPlan>).tasksMarkdown !== "string" ||
    typeof (value as Partial<CompactPlan>).testPlan !== "string" ||
    typeof (value as Partial<CompactPlan>).context !== "string"
  )
    throw corrupted("plan.json 结构无效");
  if ((value as Partial<CompactPlan>).dependencies !== undefined)
    assertPlannedDependencies((value as Partial<CompactPlan>).dependencies);
  return value as CompactPlan;
}

function assertPlannedDependencies(value: unknown): void {
  if (!Array.isArray(value))
    throw corrupted("plan.json.dependencies 必须是数组");
  const names = new Set<string>();
  value.forEach((entry, index) => {
    const path = `plan.json.dependencies[${index}]`;
    if (typeof entry !== "object" || entry === null || Array.isArray(entry))
      throw corrupted(`${path} 必须是对象`);
    const dependency = entry as Record<string, unknown>;
    for (const field of ["name", "manifest", "reason"] as const) {
      if (
        typeof dependency[field] !== "string" ||
        dependency[field].trim() === ""
      )
        throw corrupted(`${path}.${field} 必须是非空字符串`);
    }
    const manifest = String(dependency.manifest).replaceAll("\\", "/");
    if (manifest !== "package.json" && !manifest.endsWith("/package.json"))
      throw corrupted(`${path}.manifest 必须指向 package.json`);
    if (!["ADD", "UPDATE", "REMOVE"].includes(String(dependency.action)))
      throw corrupted(`${path}.action 无效`);
    if (
      !Array.isArray(dependency.requirementIds) ||
      !dependency.requirementIds.every((item) => /^REQ-\d+$/.test(String(item)))
    )
      throw corrupted(`${path}.requirementIds 必须为 Requirement ID 数组`);
    const key = `${manifest}:${dependency.name}:${dependency.action}`;
    if (names.has(key)) throw corrupted(`${path} 存在重复依赖决策`);
    names.add(key);
  });
}

async function readJson(path: string, name: string): Promise<unknown> {
  try {
    return JSON.parse(await readFile(path, "utf8"));
  } catch {
    throw corrupted(`${name} 不是有效 JSON`);
  }
}

function corrupted(message: string): SddError {
  return new SddError("E_STATE_CORRUPTED", message, "sdd status");
}
