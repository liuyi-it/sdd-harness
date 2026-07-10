import { SddError } from "../errors.js";
export function validateTaskFiles(files, scope) {
    for (const rawFile of files) {
        const file = rawFile.replaceAll("\\", "/").replace(/^\.\//, "");
        if (scope.forbiddenFiles.some((pattern) => matches(pattern, file))) {
            throw new SddError("E_SECURITY_BLOCKED", `任务修改了禁止文件：${file}`);
        }
        const allowed = [...scope.allowedFiles, ...scope.expectedNewFiles].some((pattern) => matches(pattern, file));
        if (!allowed) {
            throw new SddError("E_SECURITY_BLOCKED", `任务修改了「允许文件」范围之外的文件：${file}`);
        }
    }
}
function matches(pattern, file) {
    let expression = "^";
    for (let index = 0; index < pattern.length; index += 1) {
        const character = pattern[index];
        if (character === "*" && pattern[index + 1] === "*") {
            expression += ".*";
            index += 1;
        }
        else if (character === "*") {
            expression += "[^/]*";
        }
        else if (character === "?") {
            expression += "[^/]";
        }
        else {
            expression += character?.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") ?? "";
        }
    }
    return new RegExp(`${expression}$`).test(file);
}
//# sourceMappingURL=task-scope.js.map