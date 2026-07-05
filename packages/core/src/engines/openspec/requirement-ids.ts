export function extractRequirementIds(spec: string): string[] {
  const ids: string[] = [];
  const used = new Set(
    [...spec.matchAll(/^### REQ-(\d+)/gm)].map((match) => `REQ-${match[1]}`),
  );
  let generated = 0;
  for (const match of spec.matchAll(/^### (?:REQ-(\d+)|Requirement:)/gm)) {
    if (match[1]) ids.push(`REQ-${match[1]}`);
    else {
      let id: string;
      do {
        generated += 1;
        id = `REQ-${String(generated).padStart(3, "0")}`;
      } while (used.has(id));
      used.add(id);
      ids.push(id);
    }
  }
  return ids;
}
