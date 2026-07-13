import { parseSpec } from "../openspec/parser.js";
import { SddError } from "../../errors.js";
import { scopePatternsOverlap } from "../../security/scope-overlap.js";
import { detectProjectCommands } from "./project-commands.js";
const PHASES = ["RED", "GREEN", "REFACTOR", "VERIFY"];
const FILE_PATTERN = /(?:^|[\s`'"([])((?:(?:[A-Za-z]:)?\/{1,2})?(?:[\w@.-]+\/)*[\w@.-]+\.(?:properties|json|java|scala|swift|tsx|jsx|mjs|cjs|xml|ya?ml|kt|go|rs|py|rb|php|cs|ts|js))(?=$|[\s`'"),:\]])/gi;
const DIRECTORY_PATTERN = /(?:^|[\s`'"([])((?:(?:[A-Za-z]:)?\/{1,2})?[\w@.-]+(?:\/[\w@.-]+)+\/)(?=$|[\s`'"),:\]])/g;
export function createAtomicTasks(input) {
    const requirements = parseRequirements(input.spec);
    assertRequirements(requirements);
    const context = `${input.impact}\n${input.codebaseSummary}`;
    const migration = requiresExpandContract(input.design, input.impact);
    const files = extractPaths(context);
    const sourceFiles = files.filter(isSourceFile);
    const testFiles = files.filter(isTestFile);
    if (sourceFiles.length === 0 || testFiles.length === 0) {
        throw new SddError("E_UNRESOLVED_BLOCKER", "无法从 impact/codebaseSummary 推导精确的源码与测试文件范围，请先补充真实候选路径", "sdd plan");
    }
    const commands = detectProjectCommands(files);
    if (commands.length === 0) {
        throw new SddError("E_UNRESOLVED_BLOCKER", "无法从 impact/codebaseSummary 识别项目验证命令，请补充 package.json 或 pom.xml 路径", "sdd plan");
    }
    const sourceMapping = mapFiles(requirements, sourceFiles, context, isSourceFile);
    const testMapping = mapFiles(requirements, testFiles, context, isTestFile);
    const planned = requirements.map((requirement) => ({
        ...requirement,
        sourceFiles: sourceMapping.get(requirement.id),
        testFiles: testMapping.get(requirement.id),
    }));
    const newFiles = new Set(extractNewPaths(context));
    const tasks = [];
    const previousChains = [];
    for (const [index, requirement] of planned.entries()) {
        const ordinal = String(index + 1).padStart(3, "0");
        const scenarioIds = requirement.scenarios.map((scenario) => scenario.id);
        const overlappingVerifyIds = previousChains.flatMap((previous, previousIndex) => overlaps(requirement, previous)
            ? [`TASK-${String(previousIndex + 1).padStart(3, "0")}-VERIFY`]
            : []);
        for (const [phaseIndex, phase] of PHASES.entries()) {
            const id = `TASK-${ordinal}-${phase}`;
            const dependsOn = migration && phase === "VERIFY"
                ? PHASES.slice(0, phaseIndex).map((dependencyPhase) => `TASK-${ordinal}-${dependencyPhase}`)
                : phaseIndex > 0
                    ? [`TASK-${ordinal}-${PHASES[phaseIndex - 1]}`]
                    : overlappingVerifyIds;
            const allowedFiles = unique([
                ...requirement.sourceFiles,
                ...requirement.testFiles,
            ]);
            tasks.push({
                id,
                title: `${phaseTitle(phase)}：${requirement.title}`,
                phase,
                status: "PENDING",
                requirements: [requirement.id],
                scenarios: scenarioIds,
                dependsOn,
                allowedFiles,
                expectedNewFiles: allowedFiles.filter((file) => newFiles.has(file)),
                forbiddenFiles: [".git/**", ".env", "**/credentials*"],
                verification: commands,
                doneCriteria: doneCriteria(phase, scenarioIds),
                sliceType: migration ? migrationSliceType(phase) : "VERTICAL",
                userVisibleOutcome: `${requirement.title} 的用户可见行为通过完整验证`,
                acceptanceCriteria: doneCriteria(phase, scenarioIds),
                policyRefs: input.policyBundle?.policies ?? [],
                ...(requirement.testFiles[0] === undefined
                    ? {}
                    : { testSeam: requirement.testFiles[0] }),
            });
        }
        previousChains.push(requirement);
    }
    assertAcyclic(tasks);
    return { tasks, requirements: planned };
}
function requiresExpandContract(design, impact) {
    return /(?:expand[–-]migrate[–-]contract|schema migration|database migration|兼容迁移|数据迁移|扩展.*迁移.*收缩)/iu.test(`${design}\n${impact}`);
}
function migrationSliceType(phase) {
    if (phase === "RED")
        return "EXPAND";
    if (phase === "VERIFY")
        return "CONTRACT";
    return "MIGRATE";
}
function assertAcyclic(tasks) {
    const byId = new Map(tasks.map((task) => [task.id, task]));
    const visiting = new Set();
    const visited = new Set();
    const visit = (id) => {
        if (visiting.has(id))
            throw new SddError("E_STATE_CORRUPTED", `任务依赖图存在循环：${id}`);
        if (visited.has(id))
            return;
        visiting.add(id);
        for (const dependency of byId.get(id)?.dependsOn ?? [])
            visit(dependency);
        visiting.delete(id);
        visited.add(id);
    };
    for (const task of tasks)
        visit(task.id);
}
export function extractPaths(text) {
    const paths = [];
    for (const rawLine of text.split(/\r?\n/)) {
        const line = rawLine.replaceAll("\\", "/");
        for (const match of line.matchAll(FILE_PATTERN)) {
            const path = match[1];
            if (isSafeRelativePath(path))
                paths.push(path);
        }
        for (const match of line.matchAll(DIRECTORY_PATTERN)) {
            const directory = match[1];
            if (isSafeFocusedDirectory(directory))
                paths.push(`${directory}**`);
        }
    }
    return unique(paths);
}
function extractNewPaths(text) {
    const newPaths = [];
    for (const rawLine of text.split(/\r?\n/)) {
        const line = rawLine.replaceAll("\\", "/");
        for (const path of extractPaths(line)) {
            const rawPath = path.endsWith("/**") ? path.slice(0, -2) : path;
            const escaped = escapeRegExp(rawPath);
            if (new RegExp(`新增\\s+${escaped}(?=$|\\s)`, "i").test(line) ||
                new RegExp(`${escaped}\\s*(?:\\(new\\)|\\[new\\])`, "i").test(line))
                newPaths.push(path);
        }
    }
    return unique(newPaths);
}
function escapeRegExp(value) {
    return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
function parseRequirements(spec) {
    try {
        const document = parseSpec(spec);
        if (document.requirements.length > 0)
            return document.requirements.map((requirement) => ({
                id: requirement.id,
                title: requirement.title,
                scenarios: requirement.scenarios.map(({ id, title }) => ({
                    id,
                    title,
                })),
            }));
    }
    catch {
        // 继续解析兼容的旧 REQ 格式。
    }
    const headings = [...spec.matchAll(/^### REQ-(\d+)(?::\s*(.*))?$/gm)];
    return headings.map((heading, index) => {
        const id = `REQ-${heading[1]}`;
        const end = headings[index + 1]?.index ?? spec.length;
        const body = spec.slice((heading.index ?? 0) + heading[0].length, end);
        const scenarios = [...body.matchAll(/^#### Scenario:\s*(.+)$/gm)].map((scenario, scenarioIndex) => ({
            id: `${id}-SC-${String(scenarioIndex + 1).padStart(3, "0")}`,
            title: scenario[1].trim(),
        }));
        return {
            id,
            title: heading[2]?.trim() || id,
            scenarios,
        };
    });
}
function mapFiles(requirements, files, context, category) {
    const mapping = new Map();
    for (const requirement of requirements) {
        const explicit = context
            .split(/\r?\n/)
            .filter((line) => lineExplicitlyReferences(line, requirement))
            .flatMap((line) => extractPaths(line))
            .filter((file) => files.includes(file) && category(file));
        if (explicit.length > 0)
            mapping.set(requirement.id, unique(explicit));
    }
    const unresolved = requirements.filter((requirement) => !mapping.has(requirement.id));
    if (files.length === 1) {
        for (const requirement of unresolved)
            mapping.set(requirement.id, files);
        return mapping;
    }
    for (const file of files) {
        const owners = unresolved.filter((requirement) => requirementTokens(requirement).some((token) => file.toLowerCase().includes(token)));
        if (owners.length === 1) {
            const owner = owners[0];
            mapping.set(owner.id, [...(mapping.get(owner.id) ?? []), file]);
        }
    }
    const missing = requirements.find((requirement) => !mapping.has(requirement.id));
    if (missing !== undefined) {
        throw new SddError("E_UNRESOLVED_BLOCKER", `${missing.id} 无法可靠关联到唯一文件范围，请在 impact 中用 Requirement ID 或标题明确标注路径`, "sdd plan");
    }
    return mapping;
}
function lineExplicitlyReferences(line, requirement) {
    const ids = [...line.matchAll(/REQ-\d+/gi)].map((match) => match[0].toUpperCase());
    if (ids.length > 0)
        return ids.includes(requirement.id.toUpperCase());
    const normalized = line
        .trim()
        .replace(/^[-*]\s*/, "")
        .toLowerCase();
    const title = requirement.title.toLowerCase();
    return (normalized.startsWith(`${title}:`) ||
        normalized.startsWith(`requirement: ${title}`));
}
function requirementTokens(requirement) {
    return requirement.title
        .toLowerCase()
        .split(/[^\p{L}\p{N}]+/u)
        .filter((token) => token.length >= 3);
}
function isTestFile(file) {
    return /(^|\/)(?:test|tests|__tests__)(\/|$)|\.(?:test|spec)\.[^.]+$/i.test(file);
}
function isSourceFile(file) {
    if (isTestFile(file))
        return false;
    if (/^(?:package\.json|pom\.xml)$|\/(?:package\.json|pom\.xml)$/i.test(file))
        return false;
    return (file.endsWith("/**") ||
        /\.(?:ts|tsx|js|jsx|mjs|cjs|java|kt|go|rs|py|rb|php|cs|swift|scala)$/i.test(file));
}
function isSafeFocusedDirectory(directory) {
    if (!isSafeRelativePath(directory))
        return false;
    const segments = directory.split("/").filter(Boolean);
    return (segments.length >= 2 &&
        segments.every((segment) => segment !== "." && segment !== ".." && segment !== "**"));
}
function isSafeRelativePath(path) {
    return (!path.startsWith("/") &&
        !/^[A-Za-z]:\//.test(path) &&
        !path.startsWith("//") &&
        path.split("/").every((segment) => segment !== "." && segment !== ".."));
}
function overlaps(left, right) {
    return scopePatternsOverlap([...left.sourceFiles, ...left.testFiles], [...right.sourceFiles, ...right.testFiles]);
}
function assertRequirements(requirements) {
    if (requirements.length === 0) {
        throw new SddError("E_UNRESOLVED_BLOCKER", "规格至少需要一个 Requirement", "sdd plan");
    }
    const withoutScenario = requirements.find((requirement) => requirement.scenarios.length === 0);
    if (withoutScenario !== undefined) {
        throw new SddError("E_UNRESOLVED_BLOCKER", `${withoutScenario.id} 至少需要一个 Scenario`, "sdd plan");
    }
}
function phaseTitle(phase) {
    return {
        RED: "先写失败测试",
        GREEN: "最小实现",
        REFACTOR: "保持测试绿色并重构",
        VERIFY: "完整验证",
    }[phase];
}
function doneCriteria(phase, scenarios) {
    const scenarioText = scenarios.length === 0 ? "关联需求" : scenarios.join("、");
    return {
        RED: [`${scenarioText} 的测试已编写并以预期原因失败`],
        GREEN: [`${scenarioText} 以最小实现通过测试`],
        REFACTOR: [`${scenarioText} 在重构后保持测试通过`],
        VERIFY: [`${scenarioText} 的完整验证命令全部通过`],
    }[phase];
}
function unique(values) {
    return [...new Set(values)];
}
//# sourceMappingURL=planner.js.map