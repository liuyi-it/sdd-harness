import { SddError } from "../errors.js";
import { isCommandAllowed } from "../security/shell-policy.js";
const phases = ["RED", "GREEN", "REFACTOR", "VERIFY"];
const statuses = ["PENDING", "BUILDING", "DONE", "FAILED", "SKIPPED"];
export function parseTasks(raw) {
    const value = parseJson(raw, "tasks.json");
    if (!Array.isArray(value))
        fail("tasks.json", "必须是数组");
    const ids = new Set();
    const tasks = value.map((entry, index) => {
        const path = `tasks.json[${index}]`;
        record(entry, path);
        const id = text(entry.id, `${path}.id`);
        if (!/^TASK-[A-Z0-9][A-Z0-9-]*$/.test(id))
            fail(`${path}.id`, "任务 ID 格式无效");
        if (ids.has(id))
            fail(`${path}.id`, `重复任务 ID ${id}`);
        ids.add(id);
        if (!phases.includes(entry.phase))
            fail(`${path}.phase`, "阶段无效");
        if (!statuses.includes(String(entry.status)))
            fail(`${path}.status`, "状态无效");
        for (const key of [
            "requirements",
            "scenarios",
            "dependsOn",
            "allowedFiles",
            "expectedNewFiles",
            "forbiddenFiles",
            "verification",
            "doneCriteria",
        ]) {
            const values = strings(entry[key], `${path}.${key}`);
            if (new Set(values).size !== values.length)
                fail(`${path}.${key}`, "不得包含重复项");
        }
        entry.requirements.forEach((id, item) => {
            if (!/^REQ-\d+$/.test(id))
                fail(`${path}.requirements[${item}]`, "Requirement ID 格式无效");
        });
        entry.scenarios.forEach((id, item) => {
            if (!/^REQ-\d+-SC-\d+$/.test(id))
                fail(`${path}.scenarios[${item}]`, "Scenario ID 格式无效");
        });
        if (entry.requirements.length !== 1)
            fail(`${path}.requirements`, "必须且只能关联一个 Requirement");
        for (const key of ["allowedFiles", "expectedNewFiles", "forbiddenFiles"])
            entry[key].forEach((pattern, item) => {
                if (pattern.startsWith("/") ||
                    pattern.includes("\\") ||
                    pattern.split("/").includes("..") ||
                    /[\r\n\0]/.test(pattern))
                    fail(`${path}.${key}[${item}]`, "必须是安全相对路径模式");
            });
        entry.verification.forEach((command, item) => {
            if (!isCommandAllowed(command))
                fail(`${path}.verification[${item}]`, "命令未在允许清单内");
        });
        text(entry.title, `${path}.title`);
        optionalText(entry.userVisibleOutcome, `${path}.userVisibleOutcome`);
        optionalText(entry.testSeam, `${path}.testSeam`);
        if (entry.sliceType !== undefined &&
            !["VERTICAL", "EXPAND", "MIGRATE", "CONTRACT", "REPAIR"].includes(String(entry.sliceType)))
            fail(`${path}.sliceType`, "切片类型无效");
        if (entry.acceptanceCriteria !== undefined)
            strings(entry.acceptanceCriteria, `${path}.acceptanceCriteria`);
        return entry;
    });
    const taskIds = new Set(tasks.map((task) => task.id));
    tasks.forEach((task, index) => {
        task.dependsOn.forEach((dependency, dependencyIndex) => {
            if (dependency === task.id)
                fail(`tasks.json[${index}].dependsOn[${dependencyIndex}]`, "任务不得依赖自身");
            if (!taskIds.has(dependency))
                fail(`tasks.json[${index}].dependsOn[${dependencyIndex}]`, `不存在依赖任务 ${dependency}`);
        });
    });
    const visiting = new Set();
    const visited = new Set();
    const byId = new Map(tasks.map((task) => [task.id, task]));
    const visit = (taskId) => {
        if (visited.has(taskId))
            return;
        if (visiting.has(taskId))
            fail("tasks.json", `任务依赖图存在环：${taskId}`);
        visiting.add(taskId);
        for (const dependency of byId.get(taskId)?.dependsOn ?? [])
            visit(dependency);
        visiting.delete(taskId);
        visited.add(taskId);
    };
    tasks.forEach((task) => visit(task.id));
    return tasks;
}
function optionalText(value, path) {
    if (value !== undefined)
        text(value, path);
}
export function assertTaskResultIds(tasks, results) {
    const taskIds = new Set(tasks.map((task) => task.id));
    results.forEach((result, index) => {
        if (!taskIds.has(result.taskId))
            fail(`task-results.json[${index}].taskId`, `不存在对应任务 ${result.taskId}`);
    });
}
export function parseTaskResults(raw) {
    const value = parseJson(raw, "task-results.json");
    if (!Array.isArray(value))
        fail("task-results.json", "必须是数组");
    const ids = new Set();
    return value.map((entry, index) => {
        const path = `task-results.json[${index}]`;
        record(entry, path);
        const taskId = text(entry.taskId, `${path}.taskId`);
        if (ids.has(taskId))
            fail(`${path}.taskId`, `重复结果 ID ${taskId}`);
        ids.add(taskId);
        strings(entry.modifiedFiles, `${path}.modifiedFiles`);
        evidence(entry.tddEvidence, `${path}.tddEvidence`, true);
        evidence(entry.verification, `${path}.verification`, false);
        return entry;
    });
}
function evidence(value, path, tdd) {
    if (!Array.isArray(value))
        fail(path, "必须是数组");
    value.forEach((entry, index) => {
        const itemPath = `${path}[${index}]`;
        record(entry, itemPath);
        text(entry.command, `${itemPath}.command`);
        text(entry.output, `${itemPath}.output`);
        if (typeof entry.passed !== "boolean")
            fail(`${itemPath}.passed`, "必须是 boolean");
        if (tdd && !phases.includes(entry.phase))
            fail(`${itemPath}.phase`, "阶段无效");
        if (tdd &&
            Object.hasOwn(entry, "expectedFailure") &&
            typeof entry.expectedFailure !== "boolean")
            fail(`${itemPath}.expectedFailure`, "必须是 boolean");
    });
}
function strings(value, path) {
    if (!Array.isArray(value))
        fail(path, "必须是数组");
    return value.map((entry, index) => text(entry, `${path}[${index}]`));
}
function text(value, path) {
    if (typeof value !== "string" || value.trim().length === 0)
        fail(path, "必须是非空字符串");
    if (/[\r\n\0]/.test(value))
        fail(path, "包含非法控制字符");
    return value;
}
function record(value, path) {
    if (typeof value !== "object" || value === null || Array.isArray(value))
        fail(path, "必须是对象");
}
function parseJson(raw, path) {
    try {
        return JSON.parse(raw);
    }
    catch {
        fail(path, "不是有效 JSON");
    }
}
function fail(path, message) {
    throw new SddError("E_STATE_CORRUPTED", `${path} ${message}`);
}
//# sourceMappingURL=quality-schema.js.map