import type { ProjectConventionProfile } from "./model.js";
export declare class ProjectConventionsStore {
    readonly directory: string;
    readonly jsonPath: string;
    readonly markdownPath: string;
    constructor(root: string);
    read(): Promise<ProjectConventionProfile | null>;
    write(profile: ProjectConventionProfile): Promise<ProjectConventionProfile>;
}
//# sourceMappingURL=store.d.ts.map