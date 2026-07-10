import { mkdir, stat } from "node:fs/promises";
import { dirname, join } from "node:path";
import { SddError } from "../errors.js";
import { assertSafePath } from "../security/path-safety.js";
import { GitRunner } from "./git-runner.js";
const DEFAULT_BRANCH_PATTERN = "sdd/<change-id>";
const DEFAULT_WORKTREE_DIR = ".sdd/worktrees";
const SAFE_CHANGE_ID = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
export function normalizeGitIsolationConfig(config = {}) {
    const createWorktree = config.createWorktree === true;
    const createBranch = config.createBranch === true || createWorktree;
    return {
        createBranch,
        createWorktree,
        branchPattern: config.branchPattern ?? DEFAULT_BRANCH_PATTERN,
        worktreeDir: config.worktreeDir ?? DEFAULT_WORKTREE_DIR,
    };
}
export class GitIsolationManager {
    controlRoot;
    git;
    config;
    constructor(controlRoot, config = {}, git = new GitRunner()) {
        this.controlRoot = controlRoot;
        this.git = git;
        this.config = normalizeGitIsolationConfig(config);
    }
    async plan(changeId) {
        this.assertChangeId(changeId);
        const baselineCommit = await this.git.revParse(this.controlRoot, "HEAD");
        const branchName = this.config.createBranch
            ? this.config.branchPattern.replaceAll("<change-id>", changeId)
            : null;
        const worktreePath = this.config.createWorktree
            ? await assertSafePath(this.controlRoot, join(this.config.worktreeDir, changeId))
            : null;
        return {
            controlRoot: this.controlRoot,
            businessRoot: worktreePath ?? this.controlRoot,
            branchName,
            worktreePath,
            baselineCommit,
        };
    }
    async ensure(changeId) {
        const workspace = await this.plan(changeId);
        if (workspace.worktreePath === null || workspace.branchName === null) {
            return workspace;
        }
        const registered = (await this.git.worktreeList(this.controlRoot)).find((entry) => entry.path === workspace.worktreePath);
        const exists = await pathExists(workspace.worktreePath);
        if (!exists && registered === undefined) {
            await mkdir(dirname(workspace.worktreePath), { recursive: true });
            await this.git.worktreeAdd(this.controlRoot, workspace.worktreePath, workspace.branchName, workspace.baselineCommit);
            return workspace;
        }
        if (exists !== (registered !== undefined)) {
            throw new SddError("E_STATE_CORRUPTED", `worktree 路径与 Git 注册状态不一致：${workspace.worktreePath}`);
        }
        if (registered?.branch !== workspace.branchName) {
            throw new SddError("E_STATE_CORRUPTED", `worktree 分支不匹配：期望 ${workspace.branchName}，实际 ${registered?.branch ?? "detached"}`);
        }
        if (registered.head !== workspace.baselineCommit) {
            throw new SddError("E_STATE_CORRUPTED", `worktree HEAD 与基线不一致：期望 ${workspace.baselineCommit}，实际 ${registered.head}`);
        }
        const currentBranch = await this.git.branchCurrent(workspace.worktreePath);
        if (currentBranch !== workspace.branchName) {
            throw new SddError("E_STATE_CORRUPTED", `worktree 当前分支不匹配：期望 ${workspace.branchName}，实际 ${currentBranch || "detached"}`);
        }
        const status = await this.git.statusPorcelain(workspace.worktreePath);
        if (status.trim() !== "") {
            throw new SddError("E_CONCURRENT_RUN", `worktree 存在未提交改动，拒绝复用：${workspace.worktreePath}`);
        }
        return workspace;
    }
    assertChangeId(changeId) {
        if (!SAFE_CHANGE_ID.test(changeId)) {
            throw new SddError("E_SECURITY_BLOCKED", `非法 changeId：${changeId}`);
        }
    }
}
async function pathExists(path) {
    try {
        await stat(path);
        return true;
    }
    catch {
        return false;
    }
}
//# sourceMappingURL=manager.js.map