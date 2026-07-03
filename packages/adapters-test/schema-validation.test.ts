import { describe, expect, it } from "vitest";

import { validateSchemas } from "../../scripts/validate-schemas.mjs";

describe("schema validation script", () => {
  it("validates bundled schemas against valid and invalid samples", async () => {
    await expect(validateSchemas(process.cwd())).resolves.toBeUndefined();
  });
});
