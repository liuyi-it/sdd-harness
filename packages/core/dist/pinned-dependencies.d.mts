export const PINNED_DEPENDENCIES: {
    readonly codebaseMemoryMcp: {
        readonly name: "codebase-memory-mcp";
        readonly repository: "https://github.com/DeusData/codebase-memory-mcp";
        readonly version: "v0.8.1";
        readonly commit: "f0c9be19c5d74b84f418d807bfdce7b5d6a261ff";
        readonly license: "MIT";
        readonly interface: "mcp";
        readonly localModifications: "none";
        readonly checksumManifest: "https://github.com/DeusData/codebase-memory-mcp/releases/download/v0.8.1/checksums.txt";
        readonly checksumManifestSha256: "142399e4e552fb559ede866b2549dbacc942d56f1c8718b52bc701b21f3f94c6";
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
};
export type DeepReadonly<T> = T extends object ? { readonly [K in keyof T]: DeepReadonly<T[K]>; } : T;
//# sourceMappingURL=pinned-dependencies.d.mts.map