import type { SpecDocument, SpecScenario } from "./model.js";

export function renderSpec(document: SpecDocument): string {
  const lines = [`# ${document.title}`];
  let priorOperation: string | undefined;

  for (const requirement of document.requirements) {
    if (requirement.operation !== priorOperation) {
      lines.push("", `## ${requirement.operation} Requirements`);
      priorOperation = requirement.operation;
    }
    lines.push(
      "",
      `### Requirement: ${requirement.title}`,
      requirement.statement,
    );
    for (const scenario of requirement.scenarios)
      renderScenario(lines, scenario);
  }

  return `${lines.join("\n")}\n`;
}

function renderScenario(lines: string[], scenario: SpecScenario): void {
  lines.push("", `#### Scenario: ${scenario.title}`);
  for (const step of scenario.given) lines.push(`- GIVEN ${step}`);
  for (const step of scenario.when) lines.push(`- WHEN ${step}`);
  for (const step of scenario.then) lines.push(`- THEN ${step}`);
}
