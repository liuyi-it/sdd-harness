export function validateSpec(document) {
    const failures = [];
    const ids = new Set();
    const operationsByTitle = new Map();
    document.requirements.forEach((requirement, requirementIndex) => {
        const path = `requirements[${requirementIndex}]`;
        checkId(requirement.id, `${path}.id`, ids, failures);
        if (!/\b(?:SHALL|MUST)\b/i.test(requirement.statement)) {
            failures.push({
                code: "SPEC_NORMATIVE_KEYWORD_REQUIRED",
                path: `${path}.statement`,
                message: `Requirement “${requirement.title}” 的 statement 必须包含 SHALL 或 MUST`,
            });
        }
        if (requirement.scenarios.length === 0) {
            failures.push({
                code: "SPEC_SCENARIO_REQUIRED",
                path: `${path}.scenarios`,
                message: `Requirement “${requirement.title}” 至少需要一个 Scenario`,
            });
        }
        const normalizedTitle = requirement.title.trim().toLowerCase();
        const priorOperation = operationsByTitle.get(normalizedTitle);
        if (priorOperation && priorOperation !== requirement.operation) {
            failures.push({
                code: "SPEC_DELTA_CONFLICT",
                path: `${path}.operation`,
                message: `Requirement “${requirement.title}” 同时使用了 ${priorOperation} 和 ${requirement.operation}`,
            });
        }
        else if (!priorOperation) {
            operationsByTitle.set(normalizedTitle, requirement.operation);
        }
        requirement.scenarios.forEach((scenario, scenarioIndex) => {
            checkId(scenario.id, `${path}.scenarios[${scenarioIndex}].id`, ids, failures);
        });
    });
    return failures;
}
function checkId(id, path, ids, failures) {
    if (ids.has(id)) {
        failures.push({
            code: "SPEC_DUPLICATE_ID",
            path,
            message: `ID “${id}” 重复`,
        });
    }
    else {
        ids.add(id);
    }
}
//# sourceMappingURL=validator.js.map