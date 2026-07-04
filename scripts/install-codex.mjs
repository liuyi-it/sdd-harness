/* global console, process */

import { fileURLToPath } from "node:url";

import {
  installCodexPlugin,
  printCodexInstallSummary,
} from "./install-shared.mjs";

export { installCodexPlugin } from "./install-shared.mjs";

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  installCodexPlugin()
    .then((result) => {
      printCodexInstallSummary(result);
    })
    .catch((error) => {
      console.error(error.message);
      process.exitCode = 1;
    });
}
