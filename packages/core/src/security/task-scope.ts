import { SddError } from "../errors.js";

interface TaskScope {
  allowedFiles: string[];
  expectedNewFiles: string[];
  forbiddenFiles: string[];
}

export function validateTaskFiles(files: string[], scope: TaskScope): void {
  for (const rawFile of files) {
    const file = rawFile.replaceAll("\\", "/").replace(/^\.\//, "");
    if (scope.forbiddenFiles.some((pattern) => matches(pattern, file))) {
      throw new SddError(
        "E_SECURITY_BLOCKED",
        `Task modified forbidden file: ${file}`,
      );
    }
    const allowed = [...scope.allowedFiles, ...scope.expectedNewFiles].some(
      (pattern) => matches(pattern, file),
    );
    if (!allowed) {
      throw new SddError(
        "E_SECURITY_BLOCKED",
        `Task modified file outside Allowed Files: ${file}`,
      );
    }
  }
}

function matches(pattern: string, file: string): boolean {
  let expression = "^";
  for (let index = 0; index < pattern.length; index += 1) {
    const character = pattern[index];
    if (character === "*" && pattern[index + 1] === "*") {
      expression += ".*";
      index += 1;
    } else if (character === "*") {
      expression += "[^/]*";
    } else if (character === "?") {
      expression += "[^/]";
    } else {
      expression += character?.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") ?? "";
    }
  }
  return new RegExp(`${expression}$`).test(file);
}
