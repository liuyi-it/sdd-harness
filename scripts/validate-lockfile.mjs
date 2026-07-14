import { readFile } from "node:fs/promises";

const lockfile = JSON.parse(await readFile("package-lock.json", "utf8"));
const packages = lockfile.packages ?? {};
const rolldown = packages["node_modules/rolldown"];

if (rolldown === undefined) {
  throw new Error("package-lock.json 缺少 rolldown 依赖");
}

// npm 在已有 node_modules 时更新 lockfile，可能只保留当前平台的可选依赖。
// 这里校验两个受支持系统的全部 Rolldown bindings，阻止不完整 lockfile 进入 CI。
const supportedBindings = Object.keys(
  rolldown.optionalDependencies ?? {},
).filter(
  (name) => name.includes("binding-darwin-") || name.includes("binding-win32-"),
);
const missingBindings = supportedBindings.filter(
  (name) => packages[`node_modules/${name}`] === undefined,
);

if (supportedBindings.length === 0) {
  throw new Error(
    "package-lock.json 未声明 macOS 或 Windows 的 Rolldown bindings",
  );
}

if (missingBindings.length > 0) {
  throw new Error(
    `package-lock.json 缺少跨平台原生依赖：${missingBindings.join(", ")}。请在无 node_modules 的干净目录中重新生成 lockfile。`,
  );
}
