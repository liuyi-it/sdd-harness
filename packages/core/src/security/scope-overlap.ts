/** 比较精确文件或以 `/**` 结尾的目录范围。 */
export function scopePatternsOverlap(
  leftPatterns: string[],
  rightPatterns: string[],
): boolean {
  return leftPatterns.some((left) =>
    rightPatterns.some((right) => patternOverlaps(left, right)),
  );
}

function patternOverlaps(leftPattern: string, rightPattern: string): boolean {
  const left = describe(leftPattern);
  const right = describe(rightPattern);
  if (left.path === right.path) return true;
  if (left.directory && isChild(right.path, left.path)) return true;
  if (right.directory && isChild(left.path, right.path)) return true;
  return false;
}

function describe(pattern: string): { path: string; directory: boolean } {
  const normalized = pattern.replaceAll("\\", "/").replace(/^\.\//, "");
  const directory = normalized.endsWith("/**");
  return {
    path: (directory ? normalized.slice(0, -3) : normalized).replace(/\/$/, ""),
    directory,
  };
}

function isChild(path: string, directory: string): boolean {
  return path.startsWith(`${directory}/`);
}
