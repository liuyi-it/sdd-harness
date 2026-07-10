/** 比较精确文件或以 `/**` 结尾的目录范围。 */
export function scopePatternsOverlap(leftPatterns, rightPatterns) {
    return leftPatterns.some((left) => rightPatterns.some((right) => patternOverlaps(left, right)));
}
function patternOverlaps(leftPattern, rightPattern) {
    const left = describe(leftPattern);
    const right = describe(rightPattern);
    if (left.path === right.path)
        return true;
    if (left.directory && isChild(right.path, left.path))
        return true;
    if (right.directory && isChild(left.path, right.path))
        return true;
    return false;
}
function describe(pattern) {
    const normalized = pattern.replaceAll("\\", "/").replace(/^\.\//, "");
    const directory = normalized.endsWith("/**");
    return {
        path: (directory ? normalized.slice(0, -3) : normalized).replace(/\/$/, ""),
        directory,
    };
}
function isChild(path, directory) {
    return path.startsWith(`${directory}/`);
}
//# sourceMappingURL=scope-overlap.js.map