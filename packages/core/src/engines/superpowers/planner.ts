import { parseSpec } from "../openspec/parser.js";
import { SddError } from "../../errors.js";
import { detectProjectCommands } from "./project-commands.js";
import type { PlanningInput, TaskDefinition, TddPhase } from "./protocol.js";

interface RequirementPlan {
  id: string;
  title: string;
  scenarios: Array<{ id: string; title: string }>;
  sourceFiles: string[];
  testFiles: string[];
}

const PHASES: TddPhase[] = ["RED", "GREEN", "REFACTOR", "VERIFY"];
const PATH_PATTERN =
  /(?:^|[\s`'"(])((?:[\w@.-]+\/)*[\w@.-]+\.(?:ts|tsx|js|jsx|mjs|cjs|java|kt|go|rs|py|rb|php|cs|swift|scala|json|xml|ya?ml|properties))(?:$|[\s`'"),:])/gim;

export function createAtomicTasks(input: PlanningInput): {
  tasks: TaskDefinition[];
  requirements: RequirementPlan[];
} {
  const requirements = parseRequirements(input.spec);
  const files = extractPaths(`${input.impact}\n${input.codebaseSummary}`);
  const sourceFiles = files.filter(isSourceFile);
  const testFiles = files.filter(isTestFile);
  if (sourceFiles.length === 0 || testFiles.length === 0) {
    throw new SddError(
      "E_UNRESOLVED_BLOCKER",
      "无法从 impact/codebaseSummary 推导精确的源码与测试文件范围，请先补充真实候选路径",
      "sdd plan",
    );
  }

  const commands = detectProjectCommands(files);
  if (commands.length === 0) {
    throw new SddError(
      "E_UNRESOLVED_BLOCKER",
      "无法从 impact/codebaseSummary 识别项目验证命令，请补充 package.json 或 pom.xml 路径",
      "sdd plan",
    );
  }
  const planned = requirements.map((requirement, index) => ({
    ...requirement,
    sourceFiles: selectFiles(
      requirement,
      sourceFiles,
      index,
      requirements.length,
    ),
    testFiles: selectFiles(requirement, testFiles, index, requirements.length),
  }));
  const tasks: TaskDefinition[] = [];
  const previousChains: RequirementPlan[] = [];
  for (const [index, requirement] of planned.entries()) {
    const ordinal = String(index + 1).padStart(3, "0");
    const scenarioIds = requirement.scenarios.map((scenario) => scenario.id);
    let overlappingChain: RequirementPlan | undefined;
    for (
      let previousIndex = previousChains.length - 1;
      previousIndex >= 0;
      previousIndex -= 1
    ) {
      const previous = previousChains[previousIndex]!;
      if (overlaps(requirement, previous)) {
        overlappingChain = previous;
        break;
      }
    }
    for (const [phaseIndex, phase] of PHASES.entries()) {
      const id = `TASK-${ordinal}-${phase}`;
      const previousId =
        phaseIndex > 0
          ? `TASK-${ordinal}-${PHASES[phaseIndex - 1]}`
          : overlappingChain === undefined
            ? undefined
            : `TASK-${String(previousChains.indexOf(overlappingChain) + 1).padStart(3, "0")}-VERIFY`;
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
        dependsOn: previousId === undefined ? [] : [previousId],
        allowedFiles,
        expectedNewFiles: allowedFiles,
        forbiddenFiles: [".git/**", ".env", "**/credentials*"],
        verification: commands,
        doneCriteria: doneCriteria(phase, scenarioIds),
      });
    }
    previousChains.push(requirement);
  }
  return { tasks, requirements: planned };
}

export function extractPaths(text: string): string[] {
  const paths: string[] = [];
  for (const match of text.matchAll(PATH_PATTERN)) {
    const path = match[1]!.replaceAll("\\", "/");
    if (!path.startsWith(".") && !path.includes("/**")) paths.push(path);
  }
  return unique(paths);
}

function parseRequirements(
  spec: string,
): Omit<RequirementPlan, "sourceFiles" | "testFiles">[] {
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
  } catch {
    // 继续解析兼容的旧 REQ 格式。
  }
  const headings = [...spec.matchAll(/^### REQ-(\d+)(?::\s*(.*))?$/gm)];
  return headings.map((heading, index) => {
    const id = `REQ-${heading[1]}`;
    const end = headings[index + 1]?.index ?? spec.length;
    const body = spec.slice((heading.index ?? 0) + heading[0].length, end);
    const scenarios = [...body.matchAll(/^#### Scenario:\s*(.+)$/gm)].map(
      (scenario, scenarioIndex) => ({
        id: `${id}-SC-${String(scenarioIndex + 1).padStart(3, "0")}`,
        title: scenario[1]!.trim(),
      }),
    );
    return {
      id,
      title: heading[2]?.trim() || id,
      scenarios,
    };
  });
}

function selectFiles(
  requirement: Pick<RequirementPlan, "title">,
  files: string[],
  index: number,
  count: number,
): string[] {
  const tokens = requirement.title
    .toLowerCase()
    .split(/[^\p{L}\p{N}]+/u)
    .filter((token) => token.length >= 3);
  const matched = files.filter((file) =>
    tokens.some((token) => file.toLowerCase().includes(token)),
  );
  if (matched.length > 0) return matched;
  if (files.length >= count) return [files[index]!];
  return files;
}

function isTestFile(file: string): boolean {
  return /(^|\/)(?:test|tests|__tests__)(\/|$)|\.(?:test|spec)\.[^.]+$/i.test(
    file,
  );
}

function isSourceFile(file: string): boolean {
  if (isTestFile(file)) return false;
  if (/^(?:package\.json|pom\.xml)$|\/(?:package\.json|pom\.xml)$/i.test(file))
    return false;
  return /\.(?:ts|tsx|js|jsx|mjs|cjs|java|kt|go|rs|py|rb|php|cs|swift|scala)$/i.test(
    file,
  );
}

function overlaps(left: RequirementPlan, right: RequirementPlan): boolean {
  const rightFiles = new Set([...right.sourceFiles, ...right.testFiles]);
  return [...left.sourceFiles, ...left.testFiles].some((file) =>
    rightFiles.has(file),
  );
}

function phaseTitle(phase: TddPhase): string {
  return {
    RED: "先写失败测试",
    GREEN: "最小实现",
    REFACTOR: "保持测试绿色并重构",
    VERIFY: "完整验证",
  }[phase];
}

function doneCriteria(phase: TddPhase, scenarios: string[]): string[] {
  const scenarioText =
    scenarios.length === 0 ? "关联需求" : scenarios.join("、");
  return {
    RED: [`${scenarioText} 的测试已编写并以预期原因失败`],
    GREEN: [`${scenarioText} 以最小实现通过测试`],
    REFACTOR: [`${scenarioText} 在重构后保持测试通过`],
    VERIFY: [`${scenarioText} 的完整验证命令全部通过`],
  }[phase];
}

function unique(values: string[]): string[] {
  return [...new Set(values)];
}
