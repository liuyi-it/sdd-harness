import { readFile } from "node:fs/promises";
import { join } from "node:path";

import type { SpecDocument } from "../engines/openspec/model.js";
import type { TaskDefinition } from "../engines/superpowers/protocol.js";
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
  return value as CompactPlan;
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
