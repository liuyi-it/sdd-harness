import { isCommandAllowed } from "../../security/shell-policy.js";
/** 只依据已索引的项目清单返回受 shell-policy 约束的稳定命令。 */
export function detectProjectCommands(files) {
    const normalized = files.map((file) => file.replaceAll("\\", "/"));
    const candidates = [];
    if (normalized.some((file) => file === "pom.xml" || file.endsWith("/pom.xml")))
        candidates.push("mvn test", "mvn verify");
    if (normalized.some((file) => file === "package.json" || file.endsWith("/package.json")))
        candidates.push("npm test");
    return [...new Set(candidates)].filter(isCommandAllowed);
}
//# sourceMappingURL=project-commands.js.map