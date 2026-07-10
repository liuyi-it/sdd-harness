/**
 * 固定外部依赖的来源与版本信息，避免运行时跟随 latest 漂移。
 * init 阶段会直接把这份信息写入 `.sdd/adapters/<name>/version.json`。
 */
/**
 * @template T
 * @typedef {T extends object ? { readonly [K in keyof T]: DeepReadonly<T[K]> } : T} DeepReadonly
 */
/**
 * 让唯一的运行时定义同时导出深度只读的字面量类型。
 *
 * @template T
 * @param {T} value
 * @returns {DeepReadonly<T>}
 */
function asDeepReadonly(value) {
    return /** @type {DeepReadonly<T>} */ (value);
}
export const PINNED_DEPENDENCIES = asDeepReadonly(
/** @type {const} */ ({
    codebaseMemoryMcp: {
        name: "codebase-memory-mcp",
        repository: "https://github.com/DeusData/codebase-memory-mcp",
        version: "v0.8.1",
        commit: "f0c9be19c5d74b84f418d807bfdce7b5d6a261ff",
        license: "MIT",
        interface: "mcp",
        localModifications: "none",
        checksumManifest: "https://github.com/DeusData/codebase-memory-mcp/releases/download/v0.8.1/checksums.txt",
        checksumManifestSha256: "142399e4e552fb559ede866b2549dbacc942d56f1c8718b52bc701b21f3f94c6",
    },
    openSpec: {
        name: "openspec",
        repository: "https://github.com/Fission-AI/OpenSpec",
        version: "v1.4.1",
        commit: "1b06fddd59d8e592d5b5794a1970b22867e85b1f",
        license: "MIT",
        interface: "vendored-module",
        localModifications: "concepts reimplemented in SpecEngine; upstream source not copied into runtime",
    },
    superpowers: {
        name: "superpowers",
        repository: "https://github.com/obra/superpowers",
        version: "v6.1.1",
        commit: "d884ae04edebef577e82ff7c4e143debd0bbec99",
        license: "MIT",
        interface: "vendored-module",
        localModifications: "concepts reimplemented in TddEngine; upstream source not copied into runtime",
    },
}));
//# sourceMappingURL=pinned-dependencies.mjs.map