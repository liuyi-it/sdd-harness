export function renderSpec(document) {
    assertDocumentRenderSafe(document);
    const lines = [`# ${document.title}`];
    let priorOperation;
    for (const requirement of document.requirements) {
        if (requirement.operation !== priorOperation) {
            lines.push("", `## ${requirement.operation} Requirements`);
            priorOperation = requirement.operation;
        }
        lines.push("", `### Requirement: ${requirement.title}`, requirement.statement);
        for (const scenario of requirement.scenarios)
            renderScenario(lines, scenario);
    }
    return `${lines.join("\n")}\n`;
}
function assertDocumentRenderSafe(document) {
    assertRenderSafe(document.title, "title");
    document.requirements.forEach((requirement, requirementIndex) => {
        const requirementPath = `requirements[${requirementIndex}]`;
        assertRenderSafe(requirement.title, `${requirementPath}.title`);
        assertRenderSafe(requirement.statement, `${requirementPath}.statement`);
        assertStatementRenderSafe(requirement.statement, `${requirementPath}.statement`);
        requirement.scenarios.forEach((scenario, scenarioIndex) => {
            const scenarioPath = `${requirementPath}.scenarios[${scenarioIndex}]`;
            assertRenderSafe(scenario.title, `${scenarioPath}.title`);
            for (const key of ["given", "when", "then"]) {
                scenario[key].forEach((step, stepIndex) => {
                    assertRenderSafe(step, `${scenarioPath}.${key}[${stepIndex}]`);
                });
            }
        });
    });
}
function assertStatementRenderSafe(statement, path) {
    if (/^(?:#|-)/.test(statement.trim())) {
        throw new Error(`OpenSpec 字段 ${path} statement 不可注入 Markdown 结构`);
    }
}
function assertRenderSafe(value, path) {
    if (/[\r\n\0]/.test(value)) {
        throw new Error(`OpenSpec 字段 ${path} 不可包含 CR、LF 或 NUL`);
    }
}
function renderScenario(lines, scenario) {
    lines.push("", `#### Scenario: ${scenario.title}`);
    for (const step of scenario.given)
        lines.push(`- GIVEN ${step}`);
    for (const step of scenario.when)
        lines.push(`- WHEN ${step}`);
    for (const step of scenario.then)
        lines.push(`- THEN ${step}`);
}
//# sourceMappingURL=renderer.js.map