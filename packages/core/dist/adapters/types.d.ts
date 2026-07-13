export interface AdapterCapabilities {
    supportsSkills: boolean;
    supportsModelInvocation: boolean;
    supportsUserCommands: boolean;
    supportsReferences: boolean;
}
/** Adapter 包提供的宿主描述，不承载重复的工程规则正文。 */
export interface AdapterDescriptor {
    agent: string;
    instructionFile: string;
    commandsDir?: string;
    skillsDir?: string;
    capabilities: AdapterCapabilities;
}
/** Core 通过 Policy compiler 生成的安装清单。 */
export interface AdapterManifest extends AdapterDescriptor {
    instructionContent: string;
    commandsDir: string;
    commandTemplate: string;
    skillContent?: string;
    warnings: string[];
    degradationReason?: string;
}
//# sourceMappingURL=types.d.ts.map