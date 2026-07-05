/* global URL, console, process */

import { createHash } from "node:crypto";
import { readdir, readFile, writeFile } from "node:fs/promises";
import { relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const defaultVendorRoots = ["vendor/openspec", "vendor/superpowers"];

export async function createVendorManifest(vendorRoot) {
  const root = resolve(vendorRoot);
  const upstreamRoot = resolve(root, "upstream");
  const paths = await listFiles(upstreamRoot);
  const lines = await Promise.all(
    paths.map(async (path) => {
      const digest = createHash("sha256")
        .update(await readFile(resolve(upstreamRoot, path)))
        .digest("hex");
      return `${digest}  upstream/${path}`;
    }),
  );
  const output = `${lines.join("\n")}\n`;
  await writeFile(resolve(root, "MANIFEST.sha256"), output, "utf8");
  return output;
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
