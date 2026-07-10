import type { PlanningInput, TaskDefinition } from "./protocol.js";
interface RequirementPlan {
    id: string;
    title: string;
    scenarios: Array<{
        id: string;
        title: string;
    }>;
    sourceFiles: string[];
    testFiles: string[];
}
export declare function createAtomicTasks(input: PlanningInput): {
    tasks: TaskDefinition[];
    requirements: RequirementPlan[];
};
export declare function extractPaths(text: string): string[];
export {};
//# sourceMappingURL=planner.d.ts.map