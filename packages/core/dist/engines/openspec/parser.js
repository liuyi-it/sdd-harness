const DELTA_HEADING = /^## (ADDED|MODIFIED|REMOVED) Requirements$/;
const REQUIREMENT_HEADING = /^### Requirement:(.*)$/;
const SCENARIO_HEADING = /^#### Scenario:(.*)$/;
const STEP = /^-\s+(GIVEN|WHEN|THEN)\s+(.+)$/i;
export function parseSpec(markdown) {
    if (markdown.includes("\0"))
        throw new Error("OpenSpec 文档不可包含 NUL");
    const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
    const firstContent = lines.findIndex((line) => line.trim() !== "");
    if (firstContent < 0)
        throw new Error("OpenSpec 文档缺少一级标题");
    const titleMatch = /^# ([^#].*)$/.exec(lines[firstContent].trim());
    if (!titleMatch)
        throw new Error(`第 ${firstContent + 1} 行必须是一级文档标题`);
    const requirements = [];
    let operation;
    let requirement;
    let scenario;
    let statementLines = [];
    const finishRequirement = () => {
        if (!requirement)
            return;
        requirement.statement = statementLines.join(" ").trim();
        requirements.push(requirement);
        requirement = undefined;
        scenario = undefined;
        statementLines = [];
    };
    for (let index = firstContent + 1; index < lines.length; index += 1) {
        const line = lines[index].trim();
        if (!line)
            continue;
        const deltaMatch = DELTA_HEADING.exec(line);
        if (deltaMatch) {
            finishRequirement();
            operation = deltaMatch[1];
            continue;
        }
        const requirementMatch = REQUIREMENT_HEADING.exec(line);
        if (requirementMatch) {
            if (!operation)
                throw new Error(`第 ${index + 1} 行的 Requirement 缺少 delta 标题`);
            const requirementTitle = requirementMatch[1].trim();
            if (!requirementTitle)
                throw new Error(`第 ${index + 1} 行的 Requirement 标题不能为空`);
            finishRequirement();
            const requirementIndex = requirements.length + 1;
            requirement = {
                id: `REQ-${pad(requirementIndex)}`,
                title: requirementTitle,
                statement: "",
                operation,
                scenarios: [],
            };
            continue;
        }
        const scenarioMatch = SCENARIO_HEADING.exec(line);
        if (scenarioMatch) {
            if (!requirement)
                throw new Error(`第 ${index + 1} 行存在孤立 Scenario`);
            const scenarioTitle = scenarioMatch[1].trim();
            if (!scenarioTitle)
                throw new Error(`第 ${index + 1} 行的 Scenario 标题不能为空`);
            scenario = {
                id: `${requirement.id}-SC-${pad(requirement.scenarios.length + 1)}`,
                title: scenarioTitle,
                given: [],
                when: [],
                then: [],
            };
            requirement.scenarios.push(scenario);
            continue;
        }
        const stepMatch = STEP.exec(line);
        if (stepMatch) {
            if (!scenario)
                throw new Error(`第 ${index + 1} 行存在孤立场景步骤`);
            const keyword = stepMatch[1].toLowerCase();
            scenario[keyword].push(stepMatch[2].trim());
            continue;
        }
        if (line.startsWith("#"))
            throw new Error(`第 ${index + 1} 行的标题层级或格式非法`);
        if (line.startsWith("-"))
            throw new Error(`第 ${index + 1} 行的场景步骤格式非法`);
        if (!requirement)
            throw new Error(`第 ${index + 1} 行的内容不属于任何 Requirement`);
        if (scenario)
            throw new Error(`第 ${index + 1} 行的 Scenario 只允许包含场景步骤`);
        statementLines.push(line);
    }
    finishRequirement();
    return { title: titleMatch[1].trim(), requirements };
}
function pad(value) {
    return String(value).padStart(3, "0");
}
//# sourceMappingURL=parser.js.map