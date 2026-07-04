/* global console, process */

import { fileURLToPath } from "node:url";

import {
  prepareClaudeInstall,
  printClaudeInstallSummary,
} from "./install-shared.mjs";

export { prepareClaudeInstall } from "./install-shared.mjs";

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  prepareClaudeInstall()
    .then((plan) => {
      printClaudeInstallSummary(plan);
    })
    .catch((error) => {
      console.error(error.message);
      process.exitCode = 1;
    });
}
