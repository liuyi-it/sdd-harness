import type { ProjectConventionProfile } from "./model.js";
export declare function isEmptyProject(root: string): Promise<boolean>;
export declare function discoverProjectConventions(root: string): Promise<ProjectConventionProfile>;
export declare function createEmptyProjectProfile(root: string, strategy: "free-design" | "user-defined"): ProjectConventionProfile;
//# sourceMappingURL=scanner.d.ts.map