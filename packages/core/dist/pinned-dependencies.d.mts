export const PINNED_DEPENDENCIES: {
    readonly codebaseMemoryMcp: {
        readonly name: "codebase-memory-mcp";
        readonly repository: "https://github.com/DeusData/codebase-memory-mcp";
        readonly version: "v0.9.0";
        readonly commit: "b637e3330c96cfe452da623db068c241aaa3ec01";
        readonly license: "MIT";
        readonly interface: "mcp";
        readonly localModifications: "none";
        readonly checksumManifest: "https://github.com/DeusData/codebase-memory-mcp/releases/download/v0.9.0/checksums.txt";
        readonly checksumManifestSha256: "b7294616f22050124c8f2cf029cc9943e0b7d6e426fb9a0b95b1de9815c76e57";
    };
    readonly openSpec: {
        readonly name: "openspec";
        readonly repository: "https://github.com/Fission-AI/OpenSpec";
        readonly version: "v1.4.1";
        readonly commit: "1b06fddd59d8e592d5b5794a1970b22867e85b1f";
        readonly license: "MIT";
        readonly interface: "vendored-module";
        readonly localModifications: "concepts reimplemented in SpecEngine; upstream source not copied into runtime";
    };
    readonly superpowers: {
        readonly name: "superpowers";
        readonly repository: "https://github.com/obra/superpowers";
        readonly version: "v6.1.1";
        readonly commit: "d884ae04edebef577e82ff7c4e143debd0bbec99";
        readonly license: "MIT";
        readonly interface: "vendored-module";
        readonly localModifications: "concepts reimplemented in TddEngine; upstream source not copied into runtime";
    };
    readonly mattpocockSkills: {
        readonly name: "mattpocock-skills";
        readonly repository: "https://github.com/mattpocock/skills";
        readonly version: "main@391a270";
        readonly commit: "391a2701dd948f94f56a39f7533f8eea9a859c87";
        readonly license: "MIT";
        readonly interface: "vendored-policy-source";
        readonly localModifications: "selected engineering methods reimplemented as sdd-native policies; upstream runtime is not loaded";
    };
};
export type DeepReadonly<T> = T extends object ? { readonly [K in keyof T]: DeepReadonly<T[K]>; } : T;
//# sourceMappingURL=pinned-dependencies.d.mts.map