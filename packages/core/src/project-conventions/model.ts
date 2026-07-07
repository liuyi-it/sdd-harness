export interface ProjectConventionProfile {
  schemaVersion: "1.2.0";
  projectType: "empty" | "existing";
  strategy: "free-design" | "user-defined" | "discovered";
  directories: {
    source: string[];
    test: string[];
    assets: string[];
    config: string[];
  };
  conventions: Array<{ kind: string; value: string; evidence: string[] }>;
  unknowns: string[];
  ruleFiles: Array<{ path: string; scope: string; sha256: string }>;
  generatedAt: string;
  indexHash: string;
}
