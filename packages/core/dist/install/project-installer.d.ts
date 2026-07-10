import type { AdapterManifest } from "../adapters/types.js";
/**
 * 按选定的适配器清单安装项目集成文件。
 * 每个适配器独立安装其指令文件、commands、skills 和 rules。
 * schemas 与适配器无关，始终安装。
 */
export declare function installProjectIntegration(root: string, manifests: AdapterManifest[], _options?: {
    force?: boolean;
}): Promise<void>;
//# sourceMappingURL=project-installer.d.ts.map