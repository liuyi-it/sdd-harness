import { lstat, realpath } from "node:fs/promises";
import { isAbsolute, join, relative, resolve, sep } from "node:path";

import { SddError } from "../errors.js";

export async function assertSafePath(
  root: string,
  candidate: string,
): Promise<string> {
  const rootPath = await realpath(root);
  if (
    isAbsolute(candidate) ||
    /^[A-Za-z]:[\\/]/.test(candidate) ||
    candidate.startsWith("\\\\")
  ) {
    throw new SddError(
      "E_PATH_OUTSIDE_REPO",
      `Absolute path is not allowed: ${candidate}`,
    );
  }
  const portableCandidate = candidate.replaceAll("\\", "/");
  if (portableCandidate.split("/").includes("..")) {
    throw new SddError(
      "E_PATH_OUTSIDE_REPO",
      `Path escapes repository: ${candidate}`,
    );
  }
  const resolved = resolve(rootPath, portableCandidate);
  if (!isInside(rootPath, resolved)) {
    throw new SddError(
      "E_PATH_OUTSIDE_REPO",
      `Path escapes repository: ${candidate}`,
    );
  }
  const normalizedRelative = relative(rootPath, resolved).split(sep);
  if (normalizedRelative[0] === ".git") {
    throw new SddError(
      "E_SECURITY_BLOCKED",
      "Direct writes inside .git are forbidden",
    );
  }

  let cursor = rootPath;
  for (const segment of normalizedRelative) {
    cursor = join(cursor, segment);
    try {
      const stat = await lstat(cursor);
      if (stat.isSymbolicLink()) {
        const target = await realpath(cursor);
        if (!isInside(rootPath, target)) {
          throw new SddError(
            "E_SYMLINK_BLOCKED",
            `Symlink resolves outside repository: ${candidate}`,
          );
        }
        cursor = target;
      }
    } catch (error) {
      if (error instanceof SddError) throw error;
      if (!isMissingFile(error)) throw error;
      break;
    }
  }
  return resolved;
}

function isInside(root: string, candidate: string): boolean {
  const path = relative(root, candidate);
  return (
    path === "" ||
    (!path.startsWith(`..${sep}`) && path !== ".." && !isAbsolute(path))
  );
}

function isMissingFile(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    error.code === "ENOENT"
  );
}
