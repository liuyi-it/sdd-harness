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
  if (result.error) {
    console.error(`Error [${result.error.code}]: ${result.error.message}`);
    if (result.error.next) console.error(`  → ${result.error.next}`);
  }
}
