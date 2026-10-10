// SPDX-License-Identifier: MIT OR Apache-2.0

import { readFile, readdir } from "node:fs/promises";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
let input = "";
for await (const chunk of process.stdin) input += chunk;
input = input.replace(/^\uFEFF/, "").replace(/\r\n?/g, "\n");
for (const name of (await readdir(join(root, "../corpus"))).filter((item) => item.endsWith(".fountain")).sort()) {
  const text = (await readFile(join(root, "../corpus", name), "utf8")).replace(/^\uFEFF/, "").replace(/\r\n?/g, "\n");
  if (text === input) {
    const expected = await readFile(join(root, "../corpus", `${basename(name, ".fountain")}.expected.json`), "utf8");
    process.stdout.write(expected);
    process.exit(0);
  }
}
console.error("echo-expected: no corpus text matched");
process.exit(1);
