import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { SddError } from "../errors.js";
const executeFile = promisify(execFile);
const SHELL_OPERATORS = /(?:&&|\|\||[|;<>`$()])/;
const ALLOWED_PREFIXES = [
    ["rev-parse"],
    ["status"],
    ["branch"],
    ["worktree"],
];
export class GitRunner {
    exec;
    constructor(options = {}) {
        this.exec =
            options.exec ??
                (async (file, args, execOptions) => (await executeFile(file, args, execOptions)));
    }
    async run(cwd, args) {
        this.assertAllowed(args);
        const result = await this.exec("git", args, { cwd, encoding: "utf8" });
        return result.stdout.trim();
    }
    async branchCurrent(cwd) {
        return this.run(cwd, ["branch", "--show-current"]);
    }
    async revParse(cwd, target) {
        return this.run(cwd, ["rev-parse", target]);
    }
    async statusPorcelain(cwd) {
        return this.run(cwd, ["status", "--porcelain=v1", "--untracked-files=all"]);
    }
    async worktreeAdd(cwd, path, branch, commit) {
        await this.run(cwd, ["worktree", "add", "-b", branch, path, commit]);
    }
    async worktreeList(cwd) {
        const output = await this.run(cwd, ["worktree", "list", "--porcelain"]);
        if (output.trim() === "")
            return [];
        const blocks = output.split(/\n(?=worktree )/g);
        return blocks
            .map((block) => parseWorktreeBlock(block))
            .filter((entry) => entry !== null);
    }
    assertAllowed(args) {
        if (args.length === 0) {
            throw new SddError("E_SECURITY_BLOCKED", "Git 命令不能为空");
        }
        if (args.some((arg) => SHELL_OPERATORS.test(arg))) {
            throw new SddError("E_SECURITY_BLOCKED", "Git 命令参数包含不安全 shell 语义");
        }
        if (!ALLOWED_PREFIXES.some((prefix) => prefix.length <= args.length &&
            prefix.every((token, index) => args[index] === token))) {
            throw new SddError("E_SECURITY_BLOCKED", `Git 子命令不在允许清单内：${args[0]}`);
        }
    }
}
function parseWorktreeBlock(block) {
    const lines = block
        .split("\n")
        .map((line) => line.trim())
        .filter((line) => line.length > 0);
    const path = lines.find((line) => line.startsWith("worktree "))?.slice(9);
    const head = lines.find((line) => line.startsWith("HEAD "))?.slice(5);
    const branchRef = lines.find((line) => line.startsWith("branch "))?.slice(7);
    if (path === undefined || head === undefined)
        return null;
    return {
        path,
        head,
        branch: branchRef?.startsWith("refs/heads/") === true
            ? branchRef.slice("refs/heads/".length)
            : null,
    };
}
//# sourceMappingURL=git-runner.js.map