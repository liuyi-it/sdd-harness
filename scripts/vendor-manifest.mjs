/* global URL, console, process */

import { createHash } from "node:crypto";
import {
  lstat,
  readdir,
  readFile,
  readlink,
  writeFile,
} from "node:fs/promises";
import { relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const defaultVendorRoots = ["vendor/openspec", "vendor/superpowers"];

export async function createVendorManifest(vendorRoot) {
  const output = await computeVendorManifest(vendorRoot);
  await writeFile(resolve(vendorRoot, "MANIFEST.sha256"), output, "utf8");
  return output;
}

export async function computeVendorManifest(vendorRoot) {
  const root = resolve(vendorRoot);
  const upstreamRoot = resolve(root, "upstream");
  const paths = await listFiles(upstreamRoot);
  const lines = await Promise.all(
    paths.map(async (path) => {
      const absolutePath = resolve(upstreamRoot, path);
      const stats = await lstat(absolutePath);
      if (stats.isSymbolicLink()) {
        const target = await readlink(absolutePath);
        const digest = sha256(target);
        return `${digest}  symlink upstream/${path} -> ${JSON.stringify(target)}`;
      }
      if (!stats.isFile()) {
        throw new Error(`不支持的上游快照条目类型：upstream/${path}`);
      }
      const digest = sha256(await readFile(absolutePath));
      return `${digest}  file upstream/${path}`;
    }),
  );
  return `${lines.join("\n")}\n`;
}

function sha256(content) {
  return createHash("sha256").update(content).digest("hex");
}

async function listFiles(root) {
  const files = [];

  async function visit(directory) {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const absolutePath = resolve(directory, entry.name);
      if (entry.isDirectory()) {
        await visit(absolutePath);
      } else {
        files.push(relative(root, absolutePath).split(sep).join("/"));
      }
    }
  }

  await visit(root);
  return files.sort();
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const vendorRoots =
    process.argv.length > 2 ? process.argv.slice(2) : defaultVendorRoots;
  for (const vendorRoot of vendorRoots) {
    const output = await createVendorManifest(resolve(repoRoot, vendorRoot));
    console.log(
      `已生成 ${vendorRoot}/MANIFEST.sha256（${output.trimEnd().split("\n").length} 个文件）`,
    );
  }
}
