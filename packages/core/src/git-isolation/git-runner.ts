import { execFile } from "node:child_process";
import { promisify } from "node:util";

import { SddError } from "../errors.js";

const executeFile = promisify(execFile);
const SHELL_OPERATORS = /(?:&&|\|\||[|;<>`$()])/;

type ExecResult = Promise<{ stdout: string; stderr: string }>;
type ExecFunction = (
  file: string,
  args: string[],
  options: { cwd: string; encoding: "utf8" },
) => ExecResult;

const ALLOWED_PREFIXES = [
  ["rev-parse"],
  ["status"],
  ["branch"],
  ["worktree"],
] as const;

export interface GitWorktreeEntry {
  path: string;
  head: string;
  branch: string | null;
}

export class GitRunner {
  private readonly exec: ExecFunction;

  constructor(options: { exec?: ExecFunction } = {}) {
    this.exec =
      options.exec ??
      (async (file, args, execOptions) =>
        (await executeFile(file, args, execOptions)) as {
          stdout: string;
          stderr: string;
        });
  }

  async run(cwd: string, args: string[]): Promise<string> {
    this.assertAllowed(args);
    const result = await this.exec("git", args, { cwd, encoding: "utf8" });
    return result.stdout.trim();
  }

  async branchCurrent(cwd: string): Promise<string> {
    return this.run(cwd, ["branch", "--show-current"]);
  }

  async revParse(
    cwd: string,
    target: "HEAD" | "--show-toplevel",
  ): Promise<string> {
    return this.run(cwd, ["rev-parse", target]);
  }

  async statusPorcelain(cwd: string): Promise<string> {
    return this.run(cwd, ["status", "--porcelain=v1", "--untracked-files=all"]);
  }

  async worktreeAdd(
    cwd: string,
    path: string,
    branch: string,
    commit: string,
  ): Promise<void> {
    await this.run(cwd, ["worktree", "add", "-b", branch, path, commit]);
  }

  async worktreeList(cwd: string): Promise<GitWorktreeEntry[]> {
    const output = await this.run(cwd, ["worktree", "list", "--porcelain"]);
    if (output.trim() === "") return [];
    const blocks = output.split(/\n(?=worktree )/g);
    return blocks
      .map((block) => parseWorktreeBlock(block))
      .filter((entry): entry is GitWorktreeEntry => entry !== null);
  }

  private assertAllowed(args: string[]): void {
    if (args.length === 0) {
      throw new SddError("E_SECURITY_BLOCKED", "Git 命令不能为空");
    }
    if (args.some((arg) => SHELL_OPERATORS.test(arg))) {
      throw new SddError(
        "E_SECURITY_BLOCKED",
        "Git 命令参数包含不安全 shell 语义",
      );
    }
    if (
      !ALLOWED_PREFIXES.some(
        (prefix) =>
          prefix.length <= args.length &&
          prefix.every((token, index) => args[index] === token),
      )
    ) {
      throw new SddError(
        "E_SECURITY_BLOCKED",
        `Git 子命令不在允许清单内：${args[0]}`,
      );
    }
  }
}

function parseWorktreeBlock(block: string): GitWorktreeEntry | null {
  const lines = block
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  const path = lines.find((line) => line.startsWith("worktree "))?.slice(9);
  const head = lines.find((line) => line.startsWith("HEAD "))?.slice(5);
  const branchRef = lines.find((line) => line.startsWith("branch "))?.slice(7);
  if (path === undefined || head === undefined) return null;
  return {
    path,
    head,
    branch:
      branchRef?.startsWith("refs/heads/") === true
        ? branchRef.slice("refs/heads/".length)
        : null,
  };
}
