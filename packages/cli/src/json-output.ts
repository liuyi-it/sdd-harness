import type { CommandResult } from "@sdd-harness/core";

/** 以 --json 格式输出 CommandResult 到 stdout */
export function outputJson(result: CommandResult): void {
  const json: Record<string, unknown> = {
    ok: result.ok,
    state: result.state,
    exitCode: result.exitCode,
  };
  if (result.data !== undefined) json.data = result.data;
  if (result.changeId !== undefined) json.changeId = result.changeId;
  if (result.next !== undefined) json.next = result.next;
  if (result.warnings !== undefined) json.warnings = result.warnings;
  if (result.actionRequired !== undefined)
    json.actionRequired = result.actionRequired;
  if (result.error !== undefined) json.error = result.error;
  console.log(JSON.stringify(json, null, 2));
}

/** 以人类可读格式输出 */
export function outputText(result: CommandResult): void {
  console.log(`State: ${result.state}`);
  if (result.changeId) console.log(`Change: ${result.changeId}`);
  if (result.next) console.log(`Next: ${result.next}`);
  if (result.warnings && result.warnings.length > 0) {
    for (const w of result.warnings) {
      if (typeof w === "string") {
        console.log(`Warning: ${w}`);
      } else {
        console.log(`Warning [${w.code}]: ${w.message}`);
        if (w.next) console.log(`  → ${w.next}`);
      }
    }
  }
  if (result.actionRequired) {
    const ar = result.actionRequired;
    console.log(`\nAction Required: ${ar.type}`);
    console.log(`Task: ${ar.taskId}`);
    console.log(`Context Pack: ${ar.contextPack}`);
    console.log(`Result File: ${ar.resultFile}`);
    if (ar.allowedFiles.length > 0) {
      console.log(`\nAllowed Files:`);
      for (const f of ar.allowedFiles) console.log(`  - ${f}`);
    }
    if (ar.expectedNewFiles.length > 0) {
      console.log(`\nExpected New Files:`);
      for (const f of ar.expectedNewFiles) console.log(`  - ${f}`);
    }
    if (ar.forbiddenFiles.length > 0) {
      console.log(`\nForbidden Files:`);
      for (const f of ar.forbiddenFiles) console.log(`  - ${f}`);
    }
    if (ar.verification.length > 0) {
      console.log(`\nVerification:`);
      for (const v of ar.verification)
        console.log(`  - ${v.command} ${v.args.join(" ")}`);
    }
  }
  // events 输出（来自 auto --events）
  if (
    result.data &&
    typeof result.data === "object" &&
    "events" in result.data
  ) {
    const events = (result.data as Record<string, unknown>).events as
      | Array<Record<string, unknown>>
      | undefined;
    if (Array.isArray(events) && events.length > 0) {
      console.log(`\nLoop Events (${events.length}):`);
      for (const e of events) {
        const time =
          typeof e.createdAt === "string"
            ? new Date(e.createdAt).toLocaleTimeString()
            : "";
        console.log(
          `  [${time}] ${e.type}${e.command ? ` ${e.command}` : ""}${e.decision ? ` → ${e.decision}` : ""}`,
        );
      }
    }
  }
  // activeLoop 输出（来自 auto --loop-status 或 status --loop）
  if (
    result.data &&
    typeof result.data === "object" &&
    "activeLoop" in result.data
  ) {
    const loop = (result.data as Record<string, unknown>).activeLoop as
      | Record<string, unknown>
      | null
      | undefined;
    if (loop && typeof loop === "object") {
      console.log(`\nActive Loop:`);
      console.log(`  Loop: ${loop.loopId}`);
      console.log(`  Run: ${loop.runId}`);
      console.log(`  Status: ${loop.status}`);
      if (loop.waiting && typeof loop.waiting === "object") {
        const w = loop.waiting as Record<string, unknown>;
        console.log(
          `  Waiting: ${w.reason}${w.taskId ? ` (${w.taskId})` : ""}`,
        );
      }
    }
  }
  if (result.error) {
    console.error(`Error [${result.error.code}]: ${result.error.message}`);
    if (result.error.next) console.error(`  → ${result.error.next}`);
  }
}
